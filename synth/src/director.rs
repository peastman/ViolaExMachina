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

use crate::instrument::Instrument;
use crate::random::Random;
use crate::reverb::Reverb;
use crate::{InstrumentType, Articulation, SAMPLE_RATE};
use crate::filter::{Filter, LowpassFilter, ResonantFilter};
use std::f32::consts::PI;
use std::sync::mpsc;
use std::cell::RefCell;
use realfft::RealFftPlanner;

/// A message that can be sent to a Director.  Messages roughly correspond to MIDI events:
/// note on, note off, and various control channels.
pub enum Message {
    Reinitialize {instrument_type: InstrumentType, instrument_count: usize},
    NoteOn {note_index: i32, velocity: f32},
    NoteOff {note_index: i32},
    AllNotesOff,
    SetArticulation {articulation: Articulation},
    SetVolume {volume: f32},
    SetPitchBend {semitones: f32},
    SetVibrato {vibrato: f32},
    SetBowPosition {bow_position: f32},
    SetBowNoise {bow_noise: f32},
    SetReleaseRate {release: f32},
    SetHarmonics {harmonics: bool},
    SetMute {mute: bool},
    SetPolyphonic {polyphonic: bool},
    SetStereoWidth {width: f32},
    SetMaxInstrumentDelay {max_delay: i64}
}

/// A Transition describes some type of continuous change to the instruments.  It specifies the time
/// interval (in step indices) over which the change takes place.  The details of what is
/// changing are specified by the TransitionData.
struct Transition {
    start: i64,
    end: i64,
    data: TransitionData
}

/// A TransitionData is contained in a Transition.  It specifies what aspect of the instruments is
/// changing, and what values it is changing between.
enum TransitionData {
    EnvelopeChange {start_envelope: f32, end_envelope: f32},
    FrequencyChange {start_frequency: f32, end_frequency: f32}
}

/// This is the main class you interact with when synthesizing audio.  A Director controls a set
/// of Instruments, all of the same type, that play in unison.
///
/// When creating a Director with new(), you provide a Receiver<Message> that has been created
/// with mpsc::channel().  You control it by sending messages from the corresponding Sender.
/// The only method you call directly on it is generate(), which is used to generate samples.
/// This design allows control and generation to happen on different threads.
pub struct Director {
    divisions: RefCell<Vec<Division>>,
    pub instrument_type: InstrumentType,
    pub instrument_count: usize,
    pub articulation: Articulation,
    pub random: Random,
    pub fft_planner: RefCell<RealFftPlanner::<f32>>,
    left_filter: LowpassFilter,
    right_filter: LowpassFilter,
    left_mute_filter: LowpassFilter,
    right_mute_filter: LowpassFilter,
    apply_filter: bool,
    pub step: i64,
    steps_until_off: i32,
    pub max_instrument_delay: i64,
    pub volume: f32,
    pub tremolo_length: i64,
    pub tremolo_space: i64,
    pub bend: f32,
    pub vibrato: f32,
    pub bow_position: f32,
    pub release_rate: f32,
    pub bow_noise: f32,
    pub bow_noise_scale: f32,
    body_resonance: f32,
    pub harmonics: bool,
    pub mute: bool,
    polyphonic: bool,
    message_receiver: mpsc::Receiver<Message>,
    pub stereo_width: f32,
    reverb: Vec<Reverb>,
    pub noise_buffer: Vec<f32>,
}

pub struct Division {
    instruments: Vec<Instrument>,
    random: Random,
    steps_until_off: i32,
    current_note: i32,
    transitions: Vec<Transition>,
    instrument_delays: Vec<i64>,
    envelope: Vec<f32>,
    frequency: Vec<f32>,
    tremolo_start: Vec<i64>,
    tremolo_end: Vec<i64>,
    tremolo_volume: Vec<f32>,
    tremolo_down_bow: Vec<bool>,
    envelope_after_transitions: f32,
    frequency_after_transitions: f32,
    instrument_pan: Vec<f32>,
    noise_position: Vec<usize>,
    noise_filter: Vec<ResonantFilter>
}

impl Director {
    pub fn new(instrument_type: InstrumentType, instrument_count: usize, message_receiver: mpsc::Receiver<Message>) -> Self {
        let mut result = Self {
            divisions: RefCell::new(vec![]),
            instrument_type: instrument_type.clone(),
            instrument_count: 0,
            articulation: Articulation::Arco,
            random: Random::new(),
            fft_planner: RefCell::new(RealFftPlanner::<f32>::new()),
            left_filter: LowpassFilter::new(6500.0),
            right_filter: LowpassFilter::new(6500.0),
            left_mute_filter: LowpassFilter::new(1200.0),
            right_mute_filter: LowpassFilter::new(1200.0),
            apply_filter: true,
            step: 0,
            steps_until_off: 0,
            max_instrument_delay: 2000,
            volume: 1.0,
            tremolo_length: 3000,
            tremolo_space: 1000,
            bend: 1.0,
            vibrato: 0.4,
            bow_position: 0.5,
            release_rate: 0.5,
            bow_noise: 0.5,
            bow_noise_scale: 1.0,
            body_resonance: 0.1,
            harmonics: false,
            mute: false,
            polyphonic: false,
            message_receiver: message_receiver,
            stereo_width: 0.3,
            reverb: vec![],
            noise_buffer: parse_flac(include_bytes!("data/bow_noise.flac")),
        };
        for _ in 0..4 {
            result.divisions.borrow_mut().push(Division::new())
        }
        result.initialize_instruments(instrument_type, instrument_count);
        result
    }

    /// Initialize the set of instruments controlled by this Director.  This is called when it is first
    /// created, and again whenever a Reinitialize message is received.
    fn initialize_instruments(&mut self, instrument_type: InstrumentType, instrument_count: usize) {
        self.instrument_type = instrument_type.clone();
        self.instrument_count = instrument_count;
        self.bend = 1.0;
        match instrument_type {
            InstrumentType::Violin => {
                self.bow_noise_scale = 1.0;
                self.body_resonance = 0.18;
                self.tremolo_length = 3500;
                self.tremolo_space = 1000;
                self.left_mute_filter = LowpassFilter::new(1200.0);
                self.right_mute_filter = LowpassFilter::new(1200.0);
            }
            InstrumentType::Viola => {
                self.bow_noise_scale = 0.6;
                self.body_resonance = 0.18;
                self.tremolo_length = 3800;
                self.tremolo_space = 1000;
                self.left_mute_filter = LowpassFilter::new(800.0);
                self.right_mute_filter = LowpassFilter::new(800.0);
            }
            InstrumentType::Cello => {
                self.bow_noise_scale = 0.6;
                self.body_resonance = 0.35;
                self.tremolo_length = 4000;
                self.tremolo_space = 1000;
                self.left_mute_filter = LowpassFilter::new(400.0);
                self.right_mute_filter = LowpassFilter::new(400.0);
            }
            InstrumentType::Bass => {
                self.bow_noise_scale = 0.7;
                self.body_resonance = 0.4;
                self.tremolo_length = 4500;
                self.tremolo_space = 1000;
                self.left_mute_filter = LowpassFilter::new(200.0);
                self.right_mute_filter = LowpassFilter::new(200.0);
            }
        }
        let ir = match instrument_type {
            InstrumentType::Violin => parse_flac(include_bytes!("data/violin.flac")),
            InstrumentType::Viola => parse_flac(include_bytes!("data/viola.flac")),
            InstrumentType::Cello => parse_flac(include_bytes!("data/cello.flac")),
            InstrumentType::Bass => parse_flac(include_bytes!("data/bass.flac"))
        };
        self.reverb.clear();
        self.reverb.push(Reverb::new(&ir, &mut self.fft_planner.borrow_mut()));
        if instrument_count > 1 {
            self.reverb.push(Reverb::new(&ir, &mut self.fft_planner.borrow_mut()));
        }
        for division in self.divisions.borrow_mut().iter_mut() {
            division.initialize_instruments(self);
        }
    }

    /// Start playing a new note.
    fn note_on(&mut self, note_index: i32, velocity: f32) -> Result<(), String> {
        if note_index < self.instrument_type.lowest_note() || note_index > self.instrument_type.highest_note() {
            // The note index is outside the range of this instrument.  Ignore it.

            return Ok(());
        }
        let mut division_index = usize::MAX;
        if self.polyphonic {
            // Select a division to play the note.  First try to find one that is completely idle.

            for (i, division) in self.divisions.borrow().iter().enumerate() {
                if division.current_note == -1 && division.transitions.len() == 0 {
                    division_index = i;
                }
            }
            if division_index == usize::MAX {
                // None is idle.  Look for one that is in the process of releasing the previous note.

                for (i, division) in self.divisions.borrow().iter().enumerate() {
                    if division.current_note == -1 {
                        division_index = i;
                    }
                }
            }
        }
        else {
            // Send all notes to division 0.

            division_index = 0;
        }
        if division_index != usize::MAX {
            self.steps_until_off = 10000;
            self.divisions.borrow_mut()[division_index].note_on(note_index, velocity, self)
        }
        else {
            // No division is available.  Skip the note.

            Ok(())
        }
    }

    /// End the current note.  Because this is a monophonic instrument, note_on() automatically
    /// ends the current note as well.
    fn note_off(&mut self, note_index: i32) {
        for division in self.divisions.borrow_mut().iter_mut() {
            division.note_off(note_index, self)
        }
   }

    /// This is called repeated to generate audio data.  Each generates the two channels
    /// (left, right) for the next sample.
    pub fn generate(&mut self) -> (f32, f32) {
        // Deal with the queues of Messages and Transitions.  This only needs to be done occassionally.

        if self.step%100 == 0 {
            self.process_messages();
        }
        self.step += 1;

        // If nothing has been played for a while, we can return without doing anything.

        if self.steps_until_off == 0 {
            return (0.0, 0.0);
        }
        self.steps_until_off -= 1;

        // Loop over Instruments and generate audio for each one.

        let mut left = 0.0;
        let mut right = 0.0;
        for division in self.divisions.borrow_mut().iter_mut() {
            let (div_left, div_right) = division.generate(self);
            left += div_left;
            right += div_right;
        }
        if self.apply_filter {
            left = self.left_filter.process(left);
            right = self.right_filter.process(right);
        }
        let mut left_resonance = self.body_resonance*self.reverb[0].process(left);
        if self.mute {
            left_resonance = self.left_mute_filter.process(left_resonance);
        }
        left += left_resonance;
        if self.reverb.len() == 1 {
            right = left;
        }
        else {
            let mut right_resonance = self.body_resonance*self.reverb[1].process(right);
            if self.mute {
                right_resonance = self.right_mute_filter.process(right_resonance);
            }
            right += right_resonance;
        }
        if self.steps_until_off < 100 && (left.abs() > 0.001 || right.abs() > 0.001) {
            self.steps_until_off = 100;
        }
        let scale = 0.01/(self.instrument_count as f32).sqrt();
        (scale*left, scale*right)
    }

    /// This is called occasionally by generate().  It processes any Messages that have been
    /// received since the last call.
    fn process_messages(&mut self) {
        loop {
            match self.message_receiver.try_recv() {
                Ok(message) => {
                    match message {
                        Message::Reinitialize {instrument_type, instrument_count} => {
                            self.initialize_instruments(instrument_type, instrument_count);
                        }
                        Message::NoteOn {note_index, velocity} => {
                            let _ = self.note_on(note_index, velocity);
                        }
                        Message::NoteOff {note_index} => {
                            self.note_off(note_index);
                        }
                        Message::AllNotesOff => {
                            for division in self.divisions.borrow_mut().iter_mut() {
                                division.note_off(division.current_note, self)
                            }
                        }
                        Message::SetVolume {volume} => {
                            self.volume = volume;
                            for division in self.divisions.borrow_mut().iter_mut() {
                                division.update_volume(self);
                            }
                        }
                        Message::SetArticulation {articulation} => {
                            self.articulation = articulation;
                            self.apply_filter = match articulation {
                                Articulation::Arco => true,
                                Articulation::Marcato => true,
                                Articulation::Spiccato => true,
                                Articulation::Pizzicato => false,
                                Articulation::Tremolo => true
                            };
                        }
                        Message::SetPitchBend {semitones} => {
                            self.bend = f32::powf(2.0, semitones as f32/12.0);
                            for division in self.divisions.borrow_mut().iter_mut() {
                                division.update_frequency(self);
                            }
                        }
                        Message::SetVibrato {vibrato} => {
                            self.vibrato = vibrato;
                            for division in self.divisions.borrow_mut().iter_mut() {
                                division.update_vibrato(self);
                            }
                        }
                        Message::SetBowPosition {bow_position} => {
                            self.bow_position = bow_position;
                            for division in self.divisions.borrow_mut().iter_mut() {
                                division.update_bow_position(self);
                            }
                        }
                        Message::SetBowNoise {bow_noise} => {
                            self.bow_noise = bow_noise;
                        }
                        Message::SetReleaseRate {release} => {
                            self.release_rate = release;
                        }
                        Message::SetHarmonics {harmonics} => {
                            self.harmonics = harmonics;
                            for division in self.divisions.borrow_mut().iter_mut() {
                                division.update_harmonics(self);
                            }
                        }
                        Message::SetMute {mute} => {
                            self.mute = mute;
                            self.left_mute_filter.reset();
                            self.right_mute_filter.reset();
                        }
                        Message::SetPolyphonic {polyphonic} => {
                            self.polyphonic = polyphonic;
                        }
                        Message::SetStereoWidth {width} => {
                            self.stereo_width = width;
                            for division in self.divisions.borrow_mut().iter_mut() {
                                division.update_pan_positions(self);
                            }
                        }
                        Message::SetMaxInstrumentDelay {max_delay} => {
                            self.max_instrument_delay = max_delay;
                            for division in self.divisions.borrow_mut().iter_mut() {
                                division.update_instrument_delays(self);
                            }
                        }
                    }
                }
                Err(_) => {
                    break;
                }
            }
        }
    }
}

impl Division {
    pub fn new() -> Self {
        Self {
            instruments: vec![],
            random: Random::new(),
            steps_until_off: 0,
            current_note: -1,
            transitions: vec![],
            instrument_delays: vec![],
            envelope: vec![],
            frequency: vec![],
            tremolo_start: vec![],
            tremolo_end: vec![],
            tremolo_volume: vec![],
            tremolo_down_bow: vec![],
            envelope_after_transitions: 0.0,
            frequency_after_transitions: 0.0,
            instrument_pan: vec![],
            noise_position: vec![],
            noise_filter: vec![]
        }
    }

    /// Initialize the set of instruments controlled by this Director.  This is called when it is first
    /// created, and again whenever a Reinitialize message is received.
    fn initialize_instruments(&mut self, director: &Director) {
        self.instruments.clear();
        let instrument_count = director.instrument_count;
        for i in 0..instrument_count {
            self.instruments.push(Instrument::new(director.instrument_type, i));
        }
        self.transitions.clear();
        self.instrument_delays = vec![0; instrument_count];
        self.instrument_pan = vec![0.0; instrument_count];
        self.envelope = vec![0.0; instrument_count];
        self.frequency = vec![440.0; instrument_count];
        self.tremolo_start = vec![0; instrument_count];
        self.tremolo_end = vec![0; instrument_count];
        self.tremolo_volume = vec![1.0; instrument_count];
        self.tremolo_down_bow = vec![true; instrument_count];
        self.envelope_after_transitions = 0.0;
        self.frequency_after_transitions = 0.0;
        self.noise_position = vec![0; instrument_count];
        for i in 0..instrument_count {
            self.noise_position[i] = (director.noise_buffer.len() as f32*(i as f32+0.5*self.random.get_uniform())/instrument_count as f32) as usize;
        }
        self.noise_filter = vec![ResonantFilter::new(100.0, 100.0); instrument_count];
        self.update_pan_positions(director);
        self.update_vibrato(director);
        self.update_harmonics(director);
        self.update_volume(director);
        self.update_frequency(director);
        self.update_bow_position(director);
        self.update_instrument_delays(director);
    }

    /// Start playing a new note.
    fn note_on(&mut self, note_index: i32, velocity: f32, director: &Director) -> Result<(), String> {
        self.current_note = note_index;
        for i in 0..self.envelope.len() {
            let freq = 440.0 * f32::powf(2.0, (note_index-69) as f32/12.0);
            self.frequency[i] = freq;
            self.noise_filter[i] = ResonantFilter::new(2.0*freq, freq);
        }
        self.update_frequency(director);
        for instrument in &mut self.instruments {
            instrument.note_on(note_index, director.articulation);
        }
        match &director.articulation {
            Articulation::Arco => {
                let attack_time = 1000+(30000.0*(1.0-velocity)) as i64;
                let start_envelope = 0.5*self.envelope[0];
                self.add_envelope_transition(0, start_envelope, director);
                self.add_transition(0, attack_time, director, TransitionData::EnvelopeChange {start_envelope: start_envelope, end_envelope: 1.0});
            }
            Articulation::Marcato => {
                let attack_time = 1000+(5000.0*(1.0-velocity)) as i64;
                let peak = 1.0+3.0*velocity;
                self.add_envelope_transition(attack_time, peak, director);
                self.add_transition(attack_time, 2*attack_time, director, TransitionData::EnvelopeChange {start_envelope: peak, end_envelope: 1.0});
            }
            Articulation::Spiccato => {
                let hold_time = 2750+(self.random.get_int()%500) as i64;
                let peak = 0.05+4.0*velocity;
                self.add_envelope_transition(0, peak, director);
                self.add_transition(hold_time, 0, director, TransitionData::EnvelopeChange {start_envelope: peak, end_envelope: 0.0});

                // The bow striking the string causes a momentary shift in pitch.

                let end_frequency = self.frequency[0];
                let start_frequency = 1.02*end_frequency;
                for i in 0..self.frequency.len() {
                    self.frequency[i] = start_frequency;
                }
                self.add_transition(0, 2000, director, TransitionData::FrequencyChange {start_frequency: start_frequency, end_frequency: end_frequency});
            }
            Articulation::Pizzicato => {
                let peak = 1.0+15.0*velocity;
                self.add_envelope_transition(0, peak, director);
                let end_frequency = self.frequency[0];
                let period = SAMPLE_RATE as f32/end_frequency;
                let hold_time = (2.0*period) as i64;
                self.add_transition(hold_time, 0, director, TransitionData::EnvelopeChange {start_envelope: peak, end_envelope: 0.0});

                // Plucking the string causes a momentary shift in pitch.

                let start_frequency = 1.015*end_frequency;
                for i in 0..self.frequency.len() {
                    self.frequency[i] = start_frequency;
                }
                self.add_transition(0, hold_time, director, TransitionData::FrequencyChange {start_frequency: start_frequency, end_frequency: end_frequency});
            }
            Articulation::Tremolo => {
                self.add_envelope_transition(0, 1.0, director);
                for i in 0..self.tremolo_start.len() {
                    self.tremolo_start[i] = director.step;
                    self.tremolo_end[i] = director.step + director.tremolo_length + self.random.get_int() as i64%500;
                    self.tremolo_volume[i] = 1.0 + self.random.get_uniform();
                    self.tremolo_down_bow[i] = true;
                }
            }
        }
        Ok(())
    }

    /// End the current note.  Because this is a monophonic instrument, note_on() automatically
    /// ends the current note as well.
    fn note_off(&mut self, note_index: i32, director: &Director) {
        if note_index != self.current_note {
            return;
        }
        match &director.articulation {
            Articulation::Spiccato => {}
            Articulation::Pizzicato => {}
            _ => {
                let release_time = 1000 + (10000.0*(1.0-director.release_rate)) as i64;
                self.add_envelope_transition(release_time, 0.0, director);
            }
        }
        self.current_note = -1;
   }

    /// Add a Transition to the queue.
    fn add_transition(&mut self, delay: i64, duration: i64, director: &Director, data: TransitionData) {
        let transition = Transition { start: director.step+delay, end: director.step+delay+duration, data: data };
        match &transition.data {
            TransitionData::EnvelopeChange {start_envelope: _, end_envelope} => {
                self.envelope_after_transitions = *end_envelope;
            }
            TransitionData::FrequencyChange {start_frequency: _, end_frequency} => {
                self.frequency_after_transitions = *end_frequency;
            }
        }
        self.transitions.push(transition);
    }

    fn add_envelope_transition(&mut self, time: i64, end_envelope: f32, director: &Director) {
        // Remove all current envelope transitions.

        self.transitions.retain(|t| if let TransitionData::EnvelopeChange {..} = t.data {false} else {true});
        self.add_transition(0, time, director, TransitionData::EnvelopeChange {start_envelope: self.envelope[0], end_envelope: end_envelope});
    }

    /// This is called repeated to generate audio data.  Each generates the two channels
    /// (left, right) for the next sample.
    pub fn generate(&mut self, director: &Director) -> (f32, f32) {
        // Deal with the queues of Messages and Transitions.  This only needs to be done occassionally.

        if director.step%100 == 0 {
            self.update_transitions(director);
        }

        // If nothing has been played for a while, we can return without doing anything.

        if self.instruments[0].get_volume() > 0.0 {
            self.steps_until_off = 10000;
        }
        if self.steps_until_off == 0 {
            return (0.0, 0.0);
        }
        self.steps_until_off -= 1;

        // Loop over Instruments and generate audio for each one.

        let mut left = 0.0;
        let mut right = 0.0;
        for i in 0..self.instruments.len() {
            let mut noise = director.bow_noise_scale*director.bow_noise*self.instruments[i].get_volume()*director.noise_buffer[self.noise_position[i]];
            noise += 5e-5*self.frequency[i]*self.noise_filter[i].process(noise);
            let signal = self.instruments[i].generate(&mut director.fft_planner.borrow_mut()) + noise;
            self.noise_position[i] = (self.noise_position[i]+1)%director.noise_buffer.len();
            left += self.instrument_pan[i].cos()*signal;
            right += self.instrument_pan[i].sin()*signal;
        }
        if self.steps_until_off < 100 && (left.abs() > 0.001 || right.abs() > 0.001) {
            self.steps_until_off = 100;
        }
        (left, right)
    }

    /// This is called occasionally by generate().  It processes any Transitions in the queue,
    /// updating the instruments as appropriate.
    fn update_transitions(&mut self, director: &Director) {
        let mut volume_changed = false;
        let mut frequency_changed = false;
        for transition in &self.transitions {
            for i in 0..self.instruments.len() {
                let j = director.step-self.instrument_delays[i];
                if j >= transition.start {
                    let fraction = (j-transition.start) as f32 / (transition.end-transition.start) as f32;
                    let weight2 = if j < transition.end {0.5-0.5*(fraction*std::f32::consts::PI).cos()} else {1.0};
                    let weight1 = 1.0-weight2;
                    match &transition.data {
                        TransitionData::EnvelopeChange {start_envelope, end_envelope} => {
                            self.envelope[i] = weight1*start_envelope + weight2*end_envelope;
                            volume_changed = true;
                        }
                        TransitionData::FrequencyChange {start_frequency, end_frequency} => {
                            self.frequency[i] = weight1*start_frequency + weight2*end_frequency;
                            frequency_changed = true;
                        }
                    }
                }
            }
        }
        if let Articulation::Tremolo {} = &director.articulation {
            volume_changed = true;
            frequency_changed = true;
        }
        if volume_changed {
            self.update_volume(director);
            self.update_vibrato(director);
        }
        if frequency_changed {
            self.update_frequency(director);
        }
        self.transitions.retain(|t| director.step < t.end+director.max_instrument_delay);
    }

    /// Update the volumes of all Instruments.  This is called whenever the Director's volume or
    /// envelope is changed.
    fn update_volume(&mut self, director: &Director) {
        let actual_volume = 0.05+0.95*director.volume;
        for i in 0..self.instruments.len() {
            let mut vol = actual_volume*self.envelope[i];
            if let Articulation::Tremolo {} = &director.articulation {
                // When playing tremolo, the volume needs to change continuously.

                if director.step > self.tremolo_end[i] {
                    vol = 0.0;
                    self.tremolo_volume[i] = 1.0 + self.random.get_uniform();
                    if !self.tremolo_down_bow[i] {
                        self.tremolo_volume[i] *= 0.9;
                        self.tremolo_start[i] = director.step+(0.8*director.tremolo_space as f32) as i64;
                    }
                    else {
                        self.tremolo_start[i] = director.step+director.tremolo_space;
                    }
                    self.tremolo_end[i] = self.tremolo_start[i] + director.tremolo_length + self.random.get_int() as i64 % 500;
                    self.tremolo_down_bow[i] = !self.tremolo_down_bow[i];
                }
                else if director.step < self.tremolo_start[i] {
                    vol = 0.0;
                }
                else {
                    let x = (director.step-self.tremolo_start[i]) as f32 / (self.tremolo_end[i]-self.tremolo_start[i]) as f32;
                    vol *= self.tremolo_volume[i]*3.0*(x-x*x);
                }
            }
            self.instruments[i].set_volume(vol);
        }
    }

    /// Update the frequencies of all Instruments.  This is called whenever the Director's frequency or
    /// pitch bend is changed.
    fn update_frequency(&mut self, director: &Director) {
        for i in 0..self.instruments.len() {
            let mut freq = self.frequency[i]*director.bend;
            if let Articulation::Tremolo {} = &director.articulation {
                // When playing tremolo, the frequency needs to change continuously.

                let freq_delta = 0.02*freq*director.volume;
                let low_freq = freq-0.5*freq_delta;
                if director.step < self.tremolo_start[i] {
                    let x = (self.tremolo_start[i]-director.step) as f32 / director.tremolo_space as f32;
                    freq = low_freq+x*freq_delta;
                }
                else {
                    let x = (director.step-self.tremolo_start[i]) as f32 / (self.tremolo_end[i]-self.tremolo_start[i]) as f32;
                    freq = low_freq+x*freq_delta;
                }
            }
            self.instruments[i].set_frequency(freq);
        }
    }

    /// Update the bow position of all Instruments.  This is called whenever the Director's bow position is changed.
    fn update_bow_position(&mut self, director: &Director) {
        for i in 0..self.instruments.len() {
            self.instruments[i].set_bow_position(director.bow_position);
        }
    }
    /// Update the vibrato of all Instruments.  This is called whenever the Director's vibrato is changed.
    fn update_vibrato(&mut self, director: &Director) {
        for i in 0..self.instruments.len() {
            self.instruments[i].set_vibrato_amplitude(0.01*director.vibrato*self.envelope[i]);
        }
    }

    /// Update whether harmonics are enabled for all Instruments.
    fn update_harmonics(&mut self, director: &Director) {
        for instrument in &mut self.instruments.iter_mut() {
            instrument.set_harmonics(director.harmonics);
        }
    }

    /// Update the position each instrument is panned to.
    fn update_pan_positions(&mut self, director: &Director) {
        let instrument_count = self.instruments.len();
        if instrument_count == 1 {
            self.instrument_pan[0] = 0.25*PI;
        }
        else {
            for i in 0..instrument_count {
                self.instrument_pan[i] = 0.5*PI*(0.5 + director.stereo_width*(i as f32 / (instrument_count-1) as f32 - 0.5));
            }
        }
    }

    /// Update the delay for each instrument.
    fn update_instrument_delays(&mut self, director: &Director) {
        let instrument_count = self.instruments.len();
        if instrument_count == 1 {
            self.instrument_delays[0] = 0;
        }
        else {
            for i in 0..instrument_count {
                let index = ((i+(instrument_count/2)) % instrument_count) as i64;
                self.instrument_delays[i] = director.max_instrument_delay*index/(instrument_count-1) as i64;
            }
        }
    }
}

/// Convert a FLAC encoded sample to raw audio data.
fn parse_flac(file: &[u8]) -> Vec<f32> {
    let mut reader = claxon::FlacReader::new(file).unwrap();
    assert_eq!(48000, reader.streaminfo().sample_rate);
    assert_eq!(1, reader.streaminfo().channels);
    assert_eq!(16, reader.streaminfo().bits_per_sample);
    let mut samples = Vec::new();
    for sample in reader.samples() {
        samples.push((sample.unwrap() as f32)/32768.0);
    }
    samples
}
