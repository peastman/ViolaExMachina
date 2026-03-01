// Copyright 2026 by Peter Eastman
//
// This file is part of Viola Ex Machina.
//
// Viola Ex Machina is free software: you can redistribute it and/or modify it under the terms
// of the GNU Lesser General Public License as published by the Free Software Foundation, either
// version 2.1 of the License, or (at your option) any later version.
//
// Viola Ex Machina is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See
// the GNU Lesser General Public License for more details.
//
// You should have received a copy of the GNU Lesser General Public License along with Viola Ex Machina.
// If not, see <https://www.gnu.org/licenses/>.

use crate::{ViolaExMachinaParams, InstrumentType, Articulation};
use synth::director::Message;
use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use std::sync::{Arc, Mutex, mpsc};

#[derive(PartialEq)]
enum Panel {
    Controls,
    Help,
    About
}

pub struct UIState {
    current_panel: Panel
}

impl UIState {
    pub fn new() -> Self {
        Self {
            current_panel: Panel::Controls
        }
    }
}

pub fn draw_editor(params: Arc<ViolaExMachinaParams>, sender: Arc<Mutex<mpsc::Sender<Message>>>, state: Arc<Mutex<UIState>>) -> Option<Box<dyn Editor>> {
    create_egui_editor(
        params.editor_state.clone(),
        (),
        Default::default(),
        |_, _, _| {},
        move |ctx, setter, _queue, _state| {
            egui::CentralPanel::default().show(ctx, |ui| {
                egui::SidePanel::left("tabs").max_width(100.0).resizable(false).show_inside(ui, |ui| {
                    let mut state = state.lock().unwrap();
                    ui.vertical_centered_justified(|ui| {
                        ui.selectable_value(&mut state.current_panel, Panel::Controls, "Controls");
                        ui.selectable_value(&mut state.current_panel, Panel::Help, "Help");
                        ui.selectable_value(&mut state.current_panel, Panel::About, "About");
                    })
                });
                egui::CentralPanel::default().show_inside(ui, |ui| {
                    let state = state.lock().unwrap();
                    match state.current_panel {
                        Panel::Controls => draw_controls_panel(ui, &params, &sender, setter),
                        Panel::Help => draw_help_panel(ui),
                        Panel::About => draw_about_panel(ui)
                    }
                });
            });
        },
    )
}

fn draw_controls_panel(ui: &mut egui::Ui, params: &Arc<ViolaExMachinaParams>, sender: &Arc<Mutex<mpsc::Sender<Message>>>, setter: &ParamSetter) {
    let mut new_instrument_type = params.instrument_type.value();
    let mut new_instrument_count = params.instrument_count.value();
    let mut new_articulation = params.articulation.value();
    ui.label(egui::RichText::new("The instruments in the ensemble").italics());
    ui.add_space(5.0);
    ui.horizontal(|ui| {
        ui.label("Type");
        egui::ComboBox::from_id_salt("Type").selected_text(format!("{:?}", new_instrument_type)).show_ui(ui, |ui| {
            ui.selectable_value(&mut new_instrument_type, InstrumentType::Violin, "Violin");
            ui.selectable_value(&mut new_instrument_type, InstrumentType::Viola, "Viola");
            ui.selectable_value(&mut new_instrument_type, InstrumentType::Cello, "Cello");
            ui.selectable_value(&mut new_instrument_type, InstrumentType::Bass, "Bass");
        });
        ui.add_space(10.0);
        ui.label("Number");
        ui.add(egui::Slider::new(&mut new_instrument_count, 1..=8).handle_shape(egui::style::HandleShape::Circle));
    });
    if params.instrument_type.value() != new_instrument_type || params.instrument_count.value() != new_instrument_count {
        setter.begin_set_parameter(&params.instrument_type);
        setter.set_parameter(&params.instrument_type, new_instrument_type);
        setter.end_set_parameter(&params.instrument_type);
        setter.begin_set_parameter(&params.instrument_count);
        setter.set_parameter(&params.instrument_count, new_instrument_count);
        setter.end_set_parameter(&params.instrument_count);
        let instrument_type = match &new_instrument_type {
            InstrumentType::Violin => synth::InstrumentType::Violin,
            InstrumentType::Viola => synth::InstrumentType::Viola,
            InstrumentType::Cello => synth::InstrumentType::Cello,
            InstrumentType::Bass => synth::InstrumentType::Bass,
        };
        let _ = sender.lock().unwrap().send(Message::Reinitialize {instrument_type: instrument_type, instrument_count: new_instrument_count as usize});
    };
    ui.add_space(20.0);
    ui.label(egui::RichText::new("These controls can be mapped to MIDI CCs and automated in a DAW").italics());
    ui.add_space(5.0);
    egui::Grid::new("sliders").show(ui, |ui| {
        // The choice for articulation.

        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
            ui.label("Articulation");
        });
        egui::ComboBox::from_id_salt("Articulation").selected_text(format!("{:?}", new_articulation)).show_ui(ui, |ui| {
            ui.selectable_value(&mut new_articulation, Articulation::Arco, "Arco");
            ui.selectable_value(&mut new_articulation, Articulation::Marcato, "Marcato");
            ui.selectable_value(&mut new_articulation, Articulation::Spiccato, "Spiccato");
        });
        ui.end_row();
        if params.articulation.value() != new_articulation {
            setter.begin_set_parameter(&params.articulation);
            setter.set_parameter(&params.articulation, new_articulation);
            setter.end_set_parameter(&params.articulation);
            let articulation = match &new_articulation {
                Articulation::Arco => synth::Articulation::Arco,
                Articulation::Marcato => synth::Articulation::Marcato,
                Articulation::Spiccato => synth::Articulation::Spiccato
            };
            let _ = sender.lock().unwrap().send(Message::SetArticulation {articulation: articulation});
        };

        // The sliders

        ui.spacing_mut().slider_width = 200.0;
        draw_param_slider(ui, &params.dynamics, setter);
        draw_param_slider(ui, &params.vibrato, setter);
        draw_param_slider(ui, &params.intensity, setter);
        draw_param_slider(ui, &params.brightness, setter);
        draw_param_slider(ui, &params.attack_rate, setter);
        draw_param_slider(ui, &params.release_rate, setter);
        draw_param_slider(ui, &params.stereo_width, setter);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
            ui.label("Time Spread (ms)");
        });
        let mut spread = params.time_spread.value();
        if ui.add(egui::Slider::new(&mut spread, 0..=100).handle_shape(egui::style::HandleShape::Circle)).changed() {
            setter.begin_set_parameter(&params.time_spread);
            setter.set_parameter(&params.time_spread, spread);
            setter.end_set_parameter(&params.time_spread);
        }
        ui.end_row();
        let mut accent = params.accent.value();
        if ui.checkbox(&mut accent, "Accent").changed() {
            setter.begin_set_parameter(&params.accent);
            setter.set_parameter(&params.accent, accent);
            setter.end_set_parameter(&params.accent);
        }
    });
}

fn draw_param_slider(ui: &mut egui::Ui, param: &FloatParam, setter: &ParamSetter) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
        ui.label(param.name());
    });
    let mut value = param.value();
    if ui.add(egui::Slider::new(&mut value, 0.0..=1.0).handle_shape(egui::style::HandleShape::Circle)).changed() {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, value);
        setter.end_set_parameter(param);
    }
    ui.end_row();
}

fn draw_help_panel(ui: &mut egui::Ui) {
    let mut cache = CommonMarkCache::default();
    let text = include_str!("help.md");
    egui::ScrollArea::vertical().show(ui, |ui| {
        CommonMarkViewer::new().show(ui, &mut cache, text);
    });
}

fn draw_about_panel(ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space(30.0);
        ui.label(egui::RichText::new("Viola Ex Machina").size(36.0).italics());
        ui.label(egui::RichText::new(format!("version {}", env!("CARGO_PKG_VERSION"))).size(14.0));
        ui.label(egui::RichText::new("Copyright 2026 by Peter Eastman").size(14.0));
        ui.add_space(12.0);
        ui.hyperlink("https://github.com/peastman/ViolaExMachina");
    });
}
