// SPDX-FileCopyrightText: 2021 Softbear, Inc.
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::ui::UiStatusPlaying;
use common::entity::EntityData;
use common::util::level_to_score;
use glam::Vec2;
use yew::{function_component, html, Properties};
use yew_frontend::component::meter::Meter;
use yew_frontend::translation::{t, Translation};

#[derive(Properties, PartialEq)]
pub struct StatusProps {
    pub score: u32,
    pub fps: Option<f32>,
    pub status: UiStatusPlaying,
}

#[function_component(StatusOverlay)]
pub fn status_overlay(props: &StatusProps) -> Html {
    let t = t();
    let status = &props.status;
    let level = status.entity_type.data().level;
    let next_level = level + 1;
    let level_score = level_to_score(level);
    let next_level_score = level_to_score(next_level);
    let progress = common_util::range::map_ranges(
        props.score as f32,
        level_score as f32..next_level_score as f32,
        0.0..1.0,
        true,
    );
    html! {
        <>
            <h2 style="margin-bottom: 0.25rem;">
                {t.score(props.score)}
                {" — "}
                {format!("{:.1}kn", status.velocity.to_knots())}
                {" — "}
                {format!("{}° [{}]", status.direction.to_bearing(), status.direction.to_cardinal())}
                {" — "}
                {fmt_position(status.position)}
                if let Some(fps) = props.fps {
                    {" — "}
                    {format!("{:.1} fps", fps)}
                }
            </h2>
            if next_level <= EntityData::MAX_BOAT_LEVEL {
                <Meter value={progress}>{t.upgrade_to_level_progress((progress * 100.0) as u8, next_level as u32)}</Meter>
            }
        </>
    }
}

fn fmt_position(position: Vec2) -> String {
    fn fmt_coordinate(coordinate: f32, positive: char, negative: char) -> String {
        format!(
            "{}{}",
            coordinate.round().abs(),
            if coordinate >= 0.0 {
                positive
            } else {
                negative
            }
        )
    }
    format!(
        "({}, {})",
        fmt_coordinate(position.x, 'E', 'W'),
        fmt_coordinate(position.y, 'N', 'S')
    )
}
