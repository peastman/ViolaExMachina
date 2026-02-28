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
use crate::InstrumentType;
use std::f32::consts::PI;
use std::sync::mpsc;
use realfft::RealFftPlanner;

/// A message that can be sent to a Director.  Messages roughly correspond to MIDI events:
/// note on, note off, and various control channels.
pub enum Message {
    Reinitialize {instrument_type: InstrumentType, instrument_count: usize},
    NoteOn {note_index: i32, velocity: f32},
    NoteOff,
    SetVolume {volume: f32},
    SetPitchBend {semitones: f32},
    SetVibrato {vibrato: f32},
    SetIntensity {intensity: f32},
    SetBrightness {brightness: f32},
    SetAttackRate {attack: f32},
    SetReleaseRate {release: f32},
    SetAccent {accent: bool},
    SetStereoWidth {width: f32},
    SetMaxInstrumentDelay {max_delay: i64},
    SetRandomize {randomize: f32}
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

/// A note that is being sung.  It is described by the standard MIDI properties (note index
/// and velocity), as well as the syllable to sing it on.
struct Note {
    note_index: i32
}

/// This is the main class you interact with when synthesizing audio.  A Director controls a set
/// of Instruments, all of the same type, that play in unison.
///
/// When creating a Director with new(), you provide a Receiver<Message> that has been created
/// with mpsc::channel().  You control it by sending messages from the corresponding Sender.
/// The only method you call directly on it is generate(), which is used to generate samples.
/// This design allows control and generation to happen on different threads.
pub struct Director {
    instruments: Vec<Instrument>,
    instrument_type: InstrumentType,
    lowest_note: i32,
    highest_note: i32,
    random: Random,
    fft_planner: RealFftPlanner::<f32>,
    step: i64,
    steps_until_off: i32,
    transitions: Vec<Transition>,
    current_note: Option<Note>,
    max_instrument_delay: i64,
    instrument_delays: Vec<i64>,
    volume: f32,
    envelope: Vec<f32>,
    frequency: Vec<f32>,
    bend: f32,
    vibrato: f32,
    intensity: f32,
    brightness: f32,
    attack_rate: f32,
    release_rate: f32,
    body_resonance: f32,
    accent: bool,
    envelope_after_transitions: f32,
    frequency_after_transitions: f32,
    message_receiver: mpsc::Receiver<Message>,
    stereo_width: f32,
    instrument_pan: Vec<f32>,
    reverb: Vec<Reverb>,
    randomize: f32
}

impl Director {
    pub fn new(instrument_type: InstrumentType, instrument_count: usize, message_receiver: mpsc::Receiver<Message>) -> Self {
        let mut fft_planner = RealFftPlanner::<f32>::new();
        let mut result = Self {
            instruments: vec![],
            instrument_type: instrument_type.clone(),
            lowest_note: 0,
            highest_note: 0,
            random: Random::new(),
            fft_planner: fft_planner,
            step: 0,
            steps_until_off: 0,
            transitions: vec![],
            current_note: None,
            max_instrument_delay: 2000,
            instrument_delays: vec![],
            volume: 1.0,
            envelope: vec![],
            frequency: vec![],
            bend: 1.0,
            vibrato: 0.4,
            intensity: 0.5,
            brightness: 1.0,
            attack_rate: 0.8,
            release_rate: 0.5,
            body_resonance: 0.1,
            accent: false,
            envelope_after_transitions: 0.0,
            frequency_after_transitions: 0.0,
            message_receiver: message_receiver,
            stereo_width: 0.3,
            instrument_pan: vec![],
            reverb: vec![],
            randomize: 0.1
        };
        result.initialize_instruments(instrument_type, instrument_count);
        result
    }

    /// Initialize the set of instruments controlled by this Director.  This is called when it is first
    /// created, and again whenever a Reinitialize message is received.
    fn initialize_instruments(&mut self, instrument_type: InstrumentType, instrument_count: usize) {
        self.instrument_type = instrument_type.clone();
        self.instruments.clear();
        for i in 0..instrument_count {
            self.instruments.push(Instrument::new(instrument_type, i));
        }
        self.transitions.clear();
        self.current_note = None;
        self.instrument_delays = vec![0; instrument_count];
        self.instrument_pan = vec![0.0; instrument_count];
        self.envelope = vec![0.0; instrument_count];
        self.frequency = vec![440.0; instrument_count];
        self.bend = 1.0;
        self.envelope_after_transitions = 0.0;
        self.frequency_after_transitions = 0.0;
        self.lowest_note = instrument_type.lowest_note();
        self.highest_note = instrument_type.highest_note();
        match instrument_type {
            InstrumentType::Violin => {
                self.body_resonance = 0.18;
            }
            InstrumentType::Viola => {
                self.body_resonance = 0.18;
            }
            InstrumentType::Cello => {
                self.body_resonance = 0.35;
            }
            InstrumentType::Bass => {
                self.body_resonance = 0.5;
            }
        }
        let ir = match instrument_type {
            InstrumentType::Violin => parse_flac(include_bytes!("data/violin.flac")),
            InstrumentType::Viola => parse_flac(include_bytes!("data/viola.flac")),
            InstrumentType::Cello => parse_flac(include_bytes!("data/cello.flac")),
            InstrumentType::Bass => parse_flac(include_bytes!("data/bass.flac"))
        };
        self.reverb.clear();
        self.reverb.push(Reverb::new(&ir, &mut self.fft_planner));
        if instrument_count > 1 {
            self.reverb.push(Reverb::new(&ir, &mut self.fft_planner));
        }
        self.update_pan_positions();
        self.update_vibrato();
        self.update_volume();
        self.update_frequency();
        self.update_sound();
        self.update_instrument_delays();
    }

    /// Start playing a new note.
    fn note_on(&mut self, note_index: i32, velocity: f32) -> Result<(), String> {
        if note_index < self.lowest_note || note_index > self.highest_note {
            // The note index is outside the range of this instrument.  Ignore it.

            return Ok(());
        }
        for i in 0..self.envelope.len() {
            self.frequency[i] = 440.0 * f32::powf(2.0, (note_index-69) as f32/12.0);
        }
        self.update_frequency();
        let attack_time = 1000+(20000.0*(1.0-self.attack_rate)) as i64;
        self.add_envelope_transition(attack_time, 1.0);
        for instrument in &mut self.instruments {
            instrument.note_on(note_index);
        }
        Ok(())
    }

    /// End the current note.  Because this is a monophonic instrument, note_on() automatically
    /// ends the current note as well.
    fn note_off(&mut self) {
        let release_time = 1000 + (5000.0*(1.0-self.release_rate)) as i64;
        self.add_envelope_transition(release_time, 0.0);
   }

    /// Add a Transition to the queue.
    fn add_transition(&mut self, delay: i64, duration: i64, data: TransitionData) {
        let transition = Transition { start: self.step+delay, end: self.step+delay+duration, data: data };
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

    fn add_envelope_transition(&mut self, time: i64, end_envelope: f32) {
        // Remove all current envelope transitions.

        self.transitions.retain(|t| if let TransitionData::EnvelopeChange {..} = t.data {false} else {true});
        self.add_transition(0, time, TransitionData::EnvelopeChange {start_envelope: self.envelope[0], end_envelope: end_envelope});
    }

    /// This is called repeated to generate audio data.  Each generates the two channels
    /// (left, right) for the next sample.
    pub fn generate(&mut self) -> (f32, f32) {
        // Deal with the queues of Messages and Transitions.  This only needs to be done occassionally.

        if self.step%200 == 0 {
            self.process_messages();
            self.update_transitions();
        }
        self.step += 1;

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
            let signal = self.instruments[i].generate(self.step, &mut self.fft_planner);
            left += self.instrument_pan[i].cos()*signal;
            right += self.instrument_pan[i].sin()*signal;
        }
        left += self.body_resonance*self.reverb[0].process(left);
        if self.reverb.len() == 1 {
            right = left;
        }
        else {
            right += self.body_resonance*self.reverb[1].process(right);
        }
        if self.steps_until_off < 100 && (left.abs() > 0.001 || right.abs() > 0.001) {
            self.steps_until_off = 100;
        }
        let scale = 0.01/(self.instruments.len() as f32).sqrt();
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
                        Message::NoteOff => {
                            self.note_off();
                        }
                        Message::SetVolume {volume} => {
                            self.volume = volume;
                            self.update_volume();
                            self.update_sound();
                        }
                        Message::SetPitchBend {semitones} => {
                            self.bend = f32::powf(2.0, semitones as f32/12.0);
                            self.update_frequency();
                        }
                        Message::SetVibrato {vibrato} => {
                            self.vibrato = vibrato;
                            self.update_vibrato();
                        }
                        Message::SetIntensity {intensity} => {
                            self.intensity = intensity;
                            self.update_sound();
                        }
                        Message::SetBrightness {brightness} => {
                            self.brightness = brightness;
                        }
                        Message::SetAttackRate {attack} => {
                            self.attack_rate = attack;
                        }
                        Message::SetReleaseRate {release} => {
                            self.release_rate = release;
                        }
                        Message::SetAccent {accent} => {
                            self.accent = accent;
                        }
                        Message::SetStereoWidth {width} => {
                            self.stereo_width = width;
                            self.update_pan_positions();
                        }
                        Message::SetMaxInstrumentDelay {max_delay} => {
                            self.max_instrument_delay = max_delay;
                            self.update_instrument_delays();
                        }
                        Message::SetRandomize {randomize} => {
                            self.randomize = randomize;
                        }
                    }
                }
                Err(_) => {
                    break;
                }
            }
        }
    }

    /// This is called occasionally by generate().  It processes any Transitions in the queue,
    /// updating the instruments as appropriate.
    fn update_transitions(&mut self) {
        let mut volume_changed = false;
        let mut frequency_changed = false;
        for transition in &self.transitions {
            for i in 0..self.instruments.len() {
                let j = self.step-self.instrument_delays[i];
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
        if volume_changed {
            self.update_volume();
        }
        if frequency_changed {
            self.update_frequency();
        }
        self.transitions.retain(|t| self.step < t.end+self.max_instrument_delay);
    }

    /// Update the volumes of all Instruments.  This is called whenever the Director's volume or
    /// envelope is changed.
    fn update_volume(&mut self) {
        let actual_volume = 0.05+0.95*self.volume;
        for i in 0..self.instruments.len() {
            self.instruments[i].set_volume(actual_volume*self.envelope[i]);
        }
    }

    /// Update the frequencies of all Instruments.  This is called whenever the Director's frequency or
    /// pitch bend is changed.
    fn update_frequency(&mut self) {
        for i in 0..self.instruments.len() {
            self.instruments[i].set_frequency(self.frequency[i]*self.bend);
        }
    }

    /// Update the vibrato of all Instruments.  This is called whenever the Director's vibrato is changed.
    fn update_vibrato(&mut self) {
        let amplitude = 0.01*self.vibrato;
        let n = self.instruments.len();
        for (i, instrument) in &mut self.instruments.iter_mut().enumerate() {
            if n < 4 {
                instrument.set_vibrato_amplitude(amplitude*(1.0-0.25*i as f32));
            }
            else {
                instrument.set_vibrato_amplitude(amplitude*(1.0-0.5*(i as f32)/((n-1) as f32)));
            }
        }
    }

    /// Update Rd and noise amplitude for all instruments.  They depend on the volume and the note
    /// being played.
    fn update_sound(&mut self) {
        let noise = 0.05*(1.0-self.volume)*(1.0-self.volume);
        let tremolo = 0.2*self.intensity;
        for instrument in &mut self.instruments {
            instrument.set_noise(noise);
            instrument.set_tremolo_amplitude(tremolo);
        }
    }

    /// Update the position each instrument is panned to.
    fn update_pan_positions(&mut self) {
        let instrument_count = self.instruments.len();
        if instrument_count == 1 {
            self.instrument_pan[0] = 0.25*PI;
        }
        else {
            for i in 0..instrument_count {
                self.instrument_pan[i] = 0.5*PI*(0.5 + self.stereo_width*(i as f32 / (instrument_count-1) as f32 - 0.5));
            }
        }
    }

    /// Update the delay for each instrument.
    fn update_instrument_delays(&mut self) {
        let instrument_count = self.instruments.len();
        if instrument_count == 1 {
            self.instrument_delays[0] = 0;
        }
        else {
            for i in 0..instrument_count {
                let index = ((i+(instrument_count/2)) % instrument_count) as i64;
                self.instrument_delays[i] = self.max_instrument_delay*index/(instrument_count-1) as i64;
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
