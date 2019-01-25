use crate::objects::{Ctx, ID};
use crate::plugins::{Plugin, PluginCtx};
use dimensioned::si;
use ezgui::{Color, GfxCtx, Key};
use map_model::{
    BuildingID, IntersectionID, IntersectionType, LaneID, LaneType, PathRequest, Pathfinder,
    Position, Trace, LANE_THICKNESS,
};
use std::f64;

#[derive(Clone)]
enum Source {
    Walking(BuildingID),
    Driving(LaneID),
}

#[derive(PartialEq)]
enum Goal {
    Building(BuildingID),
    Border(IntersectionID),
}

pub struct SpawnAgent {
    from: Source,
    maybe_goal: Option<(Goal, Option<Trace>)>,
}

impl SpawnAgent {
    pub fn new(ctx: &mut PluginCtx) -> Option<SpawnAgent> {
        match ctx.primary.current_selection {
            Some(ID::Building(id)) => {
                if ctx
                    .input
                    .contextual_action(Key::F3, "spawn an agent starting here")
                {
                    return Some(SpawnAgent {
                        from: Source::Walking(id),
                        maybe_goal: None,
                    });
                }
            }
            Some(ID::Lane(id)) => {
                if ctx.primary.map.get_l(id).is_driving()
                    && ctx
                        .input
                        .contextual_action(Key::F3, "spawn an agent starting here")
                {
                    return Some(SpawnAgent {
                        from: Source::Driving(id),
                        maybe_goal: None,
                    });
                }
            }
            _ => {}
        }
        None
    }
}

impl Plugin for SpawnAgent {
    fn blocking_event(&mut self, ctx: &mut PluginCtx) -> bool {
        ctx.input.set_mode("Agent Spawner", &ctx.canvas);
        if ctx.input.modal_action("quit") {
            return false;
        }
        let map = &ctx.primary.map;

        let new_goal = match ctx.primary.current_selection {
            Some(ID::Building(b)) => Goal::Building(b),
            Some(ID::Intersection(i))
                if map.get_i(i).intersection_type == IntersectionType::Border =>
            {
                Goal::Border(i)
            }
            _ => {
                self.maybe_goal = None;
                return true;
            }
        };

        let recalculate = match self.maybe_goal {
            Some((ref g, _)) => *g == new_goal,
            None => true,
        };

        if recalculate {
            let start = match self.from {
                Source::Walking(from) => map.get_b(from).front_path.sidewalk,
                Source::Driving(from) => Position::new(from, 0.0 * si::M),
            };
            let end = match new_goal {
                Goal::Building(to) => match self.from {
                    Source::Walking(_) => map.get_b(to).front_path.sidewalk,
                    Source::Driving(_) => {
                        let end = map.find_driving_lane_near_building(to);
                        Position::new(end, map.get_l(end).length())
                    }
                },
                Goal::Border(to) => {
                    let lanes = map.get_i(to).get_incoming_lanes(
                        map,
                        match self.from {
                            Source::Walking(_) => LaneType::Sidewalk,
                            Source::Driving(_) => LaneType::Driving,
                        },
                    );
                    if lanes.is_empty() {
                        self.maybe_goal = None;
                        return true;
                    }
                    Position::new(lanes[0], map.get_l(lanes[0]).length())
                }
            };

            if let Some(path) = Pathfinder::shortest_distance(
                map,
                PathRequest {
                    start,
                    end,
                    can_use_bike_lanes: false,
                    can_use_bus_lanes: false,
                },
            ) {
                self.maybe_goal = Some((
                    new_goal,
                    path.trace(map, start.dist_along(), f64::MAX * si::M),
                ));
            } else {
                self.maybe_goal = None;
            }
        }

        if self.maybe_goal.is_some() && ctx.input.contextual_action(Key::F3, "end the agent here") {
            match (self.from.clone(), self.maybe_goal.take().unwrap().0) {
                (Source::Walking(from), Goal::Building(to)) => {
                    info!(
                        "Spawning {}",
                        ctx.primary
                            .sim
                            .seed_trip_just_walking_to_bldg(from, to, map)
                    );
                }
                (Source::Walking(from), Goal::Border(to)) => {
                    info!(
                        "Spawning {}",
                        ctx.primary
                            .sim
                            .seed_trip_just_walking_to_border(from, to, map)
                    );
                }
                (Source::Driving(from), Goal::Building(to)) => {
                    info!(
                        "Spawning {}",
                        ctx.primary
                            .sim
                            .seed_trip_with_car_appearing_to_bldg(from, to, map)
                    );
                }
                (Source::Driving(from), Goal::Border(to)) => {
                    info!(
                        "Spawning {}",
                        ctx.primary
                            .sim
                            .seed_trip_with_car_appearing_to_border(from, to, map)
                    );
                }
            };
            return false;
        }

        true
    }

    fn draw(&self, g: &mut GfxCtx, ctx: &Ctx) {
        if let Some((_, Some(ref trace))) = self.maybe_goal {
            g.draw_polygon(ctx.cs.get("route"), &trace.make_polygons(LANE_THICKNESS));
        }
    }

    fn color_for(&self, obj: ID, ctx: &Ctx) -> Option<Color> {
        match (&self.from, obj) {
            (Source::Walking(ref b1), ID::Building(b2)) if *b1 == b2 => {
                Some(ctx.cs.get("selected"))
            }
            (Source::Driving(ref l1), ID::Lane(l2)) if *l1 == l2 => Some(ctx.cs.get("selected")),
            _ => None,
        }
    }
}