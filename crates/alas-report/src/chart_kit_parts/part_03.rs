// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Scene;
    use crate::theme::PALETTE_LIGHT;

    #[test]
    fn wrap_text_keeps_every_word_and_never_exceeds_the_budget() {
        let long_message = "status=CompletedNotComparable; expected artifact (native screenshot): \
             C:/runs/optimized_aircraft.preview.png; ScreenGrab produced no output within the \
             configured timeout and the headless fallback mesh projection was also unavailable";
        let wrapped = wrap_text(long_message, 40);
        for line in wrapped.lines() {
            assert!(
                line.chars().count() <= 40 || !line.contains(' '),
                "line exceeds the budget and could have wrapped: {line:?}"
            );
        }
        let rejoined = wrapped.split_whitespace().collect::<Vec<_>>().join(" ");
        let original = long_message.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(rejoined, original, "wrapping must not drop or reorder words");
        assert!(wrapped.lines().count() > 1, "a long message should wrap onto multiple rows");
    }

    #[test]
    fn wrap_text_preserves_caller_formatted_paragraph_breaks() {
        let message = "first clause\nsecond clause";
        assert_eq!(wrap_text(message, 80), "first clause\nsecond clause");
    }

    #[test]
    fn wrap_text_keeps_an_overlong_single_word_intact() {
        let path = "C:/very/long/path/that/exceeds/the/per-line/character/budget/artifact.png";
        assert_eq!(wrap_text(path, 10), path);
    }

    #[test]
    fn colorbar_emits_gradient_border_and_three_ticks_plus_label() {
        let mut scene = Scene::new(200.0, 200.0, None);
        draw_colorbar(
            &mut scene,
            (10.0, 10.0, 20.0, 100.0),
            Colormap::Viridis,
            0.0,
            1.0,
            "Mach",
            &PALETTE_LIGHT,
        );
        let rects = scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Rect { .. }))
            .count();
        let texts = scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Text { .. }))
            .count();
        assert_eq!(rects, 65); // 64 gradient segments + 1 border
        assert_eq!(texts, 4); // 3 ticks + 1 rotated label
    }

    #[test]
    fn colorbar_label_is_horizontal_and_above_the_gradient() {
        let mut scene = Scene::new(220.0, 200.0, None);
        draw_colorbar(
            &mut scene,
            (10.0, 10.0, 20.0, 100.0),
            Colormap::Viridis,
            215_000.0,
            254_000.0,
            "Total Mass (kg)",
            &PALETTE_LIGHT,
        );
        let mut label = None;
        for element in &scene.elements {
            if let SceneElement::Text {
                text,
                pos,
                angle_deg,
                ..
            } = element
            {
                if text == "Total Mass (kg)" {
                    label = Some((*pos, *angle_deg));
                }
            }
        }
        assert_eq!(label, Some(([20.0, 4.0], 0.0)));
    }

    #[test]
    fn legend_emits_one_swatch_and_label_per_entry() {
        let mut scene = Scene::new(200.0, 200.0, None);
        let entries = vec![
            (
                "baseline".to_owned(),
                LegendMarker::Line(Stroke::new(Color::rgb(0, 0, 0), 1.0)),
            ),
            (
                "optimized".to_owned(),
                LegendMarker::Circle(Color::rgb(255, 0, 0)),
            ),
        ];
        draw_legend(&mut scene, [5.0, 5.0], &entries, &PALETTE_LIGHT, 10.0);
        assert_eq!(scene.elements.len(), 4); // 2 markers + 2 text labels
        assert!(matches!(
            scene.elements.last(),
            Some(SceneElement::Text { text, .. }) if text == "Optimized"
        ));
    }

    #[test]
    fn horizontal_legend_keeps_labels_unrotated_and_capitalized() {
        let mut scene = Scene::new(320.0, 100.0, None);
        draw_horizontal_legend(
            &mut scene,
            [10.0, 70.0],
            &[(
                "best so far".to_owned(),
                LegendMarker::Line(Stroke::new(Color::rgb(0, 0, 0), 1.0)),
            )],
            &PALETTE_LIGHT,
            10.0,
        );
        assert!(scene.elements.iter().any(|element| matches!(
            element,
            SceneElement::Text { text, angle_deg, .. }
                if text == "Best so far" && *angle_deg == 0.0
        )));
    }

    #[test]
    fn horizontal_legend_wraps_a_long_label_inside_the_canvas() {
        let mut scene = Scene::new(180.0, 100.0, None);
        draw_horizontal_legend(
            &mut scene,
            [10.0, 70.0],
            &[(
                "minimum nose load needed for steering authority".to_owned(),
                LegendMarker::Line(Stroke::new(Color::rgb(0, 0, 0), 1.0)),
            )],
            &PALETTE_LIGHT,
            10.0,
        );
        assert!(scene.elements.iter().any(|element| matches!(
            element,
            SceneElement::Text { text, angle_deg, .. }
                if text.contains('\n') && *angle_deg == 0.0
        )));
    }

    #[test]
    fn axes_emit_grid_ticks_numeric_labels_and_axis_labels() {
        let axes = Axes2D::new((60.0, 20.0, 240.0, 150.0), (0.0, 100.0), (-2.0, 8.0));
        let mut scene = Scene::new(360.0, 240.0, None);
        draw_axes(
            &axes,
            &mut scene,
            &PALETTE_LIGHT,
            Some("Distance [m]"),
            Some("Load [kN]"),
        );

        let lines = scene
            .elements
            .iter()
            .filter(|element| matches!(element, SceneElement::Line { .. }))
            .count();
        let labels = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        // Six-ish vertical and horizontal grids, with a tick mark for every
        // numeric label, proves this is more than the old rectangle-only
        // frame. Exact counts intentionally remain flexible as nice-step
        // rounding changes with chart dimensions.
        assert!(lines >= 12);
        assert!(labels.contains(&"Distance [m]"));
        assert!(labels.contains(&"Load [kN]"));
        assert!(labels.contains(&"0"));
        assert!(labels.contains(&"4"));
    }

    #[test]
    fn logarithmic_axes_use_decade_tick_labels() {
        let axes = Axes2D::new((30.0, 20.0, 240.0, 120.0), (1.0, 1_000.0), (0.0, 1.0))
            .with_x_scale(Scale::Log10);
        let mut scene = Scene::new(300.0, 180.0, None);
        draw_axes(&axes, &mut scene, &PALETTE_LIGHT, None, None);
        let labels = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(labels.contains(&"1"));
        assert!(labels.contains(&"100"));
        assert!(labels.contains(&"1000"));
    }

    #[test]
    fn fixed_y_tick_precision_keeps_two_decimal_coefficient_labels() {
        let axes =
            Axes2D::new((60.0, 20.0, 240.0, 150.0), (0.0, 1.0), (0.0, 0.2)).with_y_tick_decimals(2);
        let mut scene = Scene::new(360.0, 240.0, None);
        draw_axes(&axes, &mut scene, &PALETTE_LIGHT, None, Some("CDtot"));

        let labels = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { text, pos, .. } if pos[0] < axes.left => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(labels.contains(&"0.00"));
        assert!(labels.contains(&"0.10"));
        assert!(!labels.contains(&"0.1"));
    }

    #[test]
    fn chart_chrome_supports_titles_annotations_categories_and_responsive_margins() {
        let margins = ChartMargins::for_canvas(320.0, 220.0);
        let rect = margins.plot_rect(320.0, 220.0);
        let axes = Axes2D::new(rect, (0.0, 2.0), (0.0, 1.0));
        let mut scene = Scene::new(320.0, 220.0, None);
        draw_title(&mut scene, "Mission profile", &PALETTE_LIGHT);
        draw_annotation(&axes, &mut scene, "cruise", (1.0, 0.5), &PALETTE_LIGHT);
        draw_categorical_x_axis(&axes, &mut scene, &["OEW", "MZFW", "MTOW"], &PALETTE_LIGHT);
        assert!(scene.elements.iter().any(|element| matches!(
            element,
            SceneElement::Text { text, bold: true, .. } if text == "Mission profile"
        )));
        assert!(scene.elements.iter().any(|element| matches!(
            element,
            SceneElement::Text { text, .. } if text == "cruise"
        )));
        assert!(scene.elements.iter().any(|element| matches!(
            element,
            SceneElement::Text { text, .. } if text == "MZFW"
        )));
    }
}
