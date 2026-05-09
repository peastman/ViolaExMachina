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

use std::f32::consts::PI;
use std::sync::Arc;
use crate::random::Random;
use crate::filter::{Filter, LowpassFilter};
use crate::{InstrumentType, Articulation};
use crate::SAMPLE_RATE;
use realfft::{RealFftPlanner, ComplexToReal};
use rustfft::num_complex::Complex;

/// This struct combines a glottal source and two waveguides to form the complete synthesis model.
/// In addition, consonants can be synthesized by injecting extra noise at an arbitrary point in
/// the vocal tract.
pub struct Instrument {
    instrument_type: InstrumentType,
    spectrum_coeff: (f32, f32, f32, f32),
    volume: f32,
    frequency: f32,
    bow_position: f32,
    phase_shift: f32,
    harmonics: bool,
    vibrato_low_frequency: f32,
    vibrato_high_frequency: f32,
    vibrato_amplitude: f32,
    vibrato_frequency_drift_amplitude: f32,
    vibrato_amplitude_drift_amplitude: f32,
    vibrato_phase: f32,
    vibrato_amplitude_drift: f32,
    frequency_drift_amplitude: f32,
    frequency_drift: f32,
    pizzicato_exponent: i32,
    sample_rate: f32,
    spectrum_buffer: Vec<Complex<f32>>,
    spectrum_temp: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    output_buffer: Vec<f32>,
    spectrum_size: usize,
    output_size: usize,
    output_position: usize,
    period: f32,
    period_offset: f32,
    random: Random,
    lowpass: LowpassFilter,
    decaying_notes: Vec<DecayingNote>,
    start_new_note: bool,
    last_note: i32,
    last_articulation: Articulation
}

impl Instrument {
    pub fn new(instrument_type: InstrumentType, index: usize) -> Self {
        let vibrato_low_frequency;
        let vibrato_high_frequency;
        let spectrum_coeff;
        let pizzicato_exponent;
        let sample_rate;
        let phase_shift: f32;
        match instrument_type {
            InstrumentType::Violin => {
                vibrato_low_frequency = 5.15;
                vibrato_high_frequency = 5.4;
                spectrum_coeff = (0.18917376, 0.0050823237, 1.3494484, -2.9302601);
                pizzicato_exponent = 20;
                sample_rate = 2.0*SAMPLE_RATE as f32;
                phase_shift = 0.65;
            }
            InstrumentType::Viola => {
                vibrato_low_frequency = 5.15;
                vibrato_high_frequency = 5.4;
                spectrum_coeff = (0.59566957, -2.227358, 0.96543276, -2.170609);
                pizzicato_exponent = 20;
                sample_rate = SAMPLE_RATE as f32;
                phase_shift = 0.35;
            }
            InstrumentType::Cello => {
                vibrato_low_frequency = 5.0;
                vibrato_high_frequency = 5.5;
                spectrum_coeff = (-0.0037527911, 0.55326806, 2.1803675, -4.7307866);
                pizzicato_exponent = 30;
                sample_rate = SAMPLE_RATE as f32;
                phase_shift = 0.0;
            }
            InstrumentType::Bass => {
                vibrato_low_frequency = 4.9;
                vibrato_high_frequency = 5.4;
                spectrum_coeff = (0.34754488, -0.852053, 1.0190364, -2.5493735);
                pizzicato_exponent = 40;
                sample_rate = SAMPLE_RATE as f32;
                phase_shift = 0.0;
            }
        }
        let mut random = Random::new();
        Self {
            instrument_type: instrument_type,
            spectrum_coeff: spectrum_coeff,
            volume: 1.0,
            frequency: 440.0,
            bow_position: 0.5,
            phase_shift: phase_shift,
            harmonics: false,
            vibrato_low_frequency: vibrato_low_frequency,
            vibrato_high_frequency: vibrato_high_frequency,
            vibrato_amplitude: 0.0,
            vibrato_frequency_drift_amplitude: 0.1,
            vibrato_amplitude_drift_amplitude: 0.4,
            vibrato_phase: 0.3*index as f32,
            vibrato_amplitude_drift: random.get_normal(),
            frequency_drift_amplitude: 0.002,
            frequency_drift: random.get_normal(),
            pizzicato_exponent: pizzicato_exponent,
            sample_rate: sample_rate,
            spectrum_buffer: vec![],
            spectrum_temp: vec![],
            scratch: vec![],
            output_buffer: vec![],
            spectrum_size: 0,
            output_size: 0,
            output_position: 0,
            period: 0.0,
            period_offset: 0.0,
            random: random,
            lowpass: LowpassFilter::new(5000.0),
            decaying_notes: vec![],
            start_new_note: false,
            last_note: 0,
            last_articulation: Articulation::Arco
        }
    }

    /// Signal the start of a new note.
    pub fn note_on(&mut self, note: i32, articulation: Articulation) {
        if note != self.last_note {
            self.start_new_note = true;
            self.last_note = note;
        }
        self.last_articulation = articulation;
        self.update_filter();
    }

    /// Get the volume of the excitation from the bow (between 0.0 and 1.0).
    pub fn get_volume(&self) -> f32 {
        self.volume
    }

    /// Set the volume of the excitation from the bow (between 0.0 and 1.0).
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
    }

    /// Set the frequency of the string (in Hz).
    pub fn set_frequency(&mut self, frequency: f32) {
        self.frequency = frequency;
    }

    /// Set the bow position along the string.
    pub fn set_bow_position(&mut self, bow_position: f32) {
        self.bow_position = bow_position;
    }

    /// Set the amplitude of vibrato.
    pub fn set_vibrato_amplitude(&mut self, amplitude: f32) {
        self.vibrato_amplitude = amplitude;
    }

    /// Set whether harmonics are enabled.
    pub fn set_harmonics(&mut self, harmonics: bool) {
        self.harmonics = harmonics;
        self.update_filter();
    }

    /// Update the cutoff frequency of the lowpass filter.
    fn update_filter(&mut self) {
        let note_freq = 440.0 * f32::powf(2.0, (self.last_note-69) as f32/12.0);
        let multiplier = if self.harmonics {4.0} else {1.0};
        let cutoff = f32::min(12.0*note_freq*multiplier, 20000.0)*(SAMPLE_RATE as f32/self.sample_rate);
        self.lowpass.set_cutoff(cutoff);
    }

    /// Add excitation from the bow to the spectrum.
    fn add_bow_excitation(&mut self) {
        let mut c = self.volume/(self.spectrum_size as f32).sqrt();
        if self.harmonics {
            c *= 2.0;
        }
        if let Articulation::ColLegno {} = &self.last_articulation {
            // Reduce the pitched part of the sound.

            c *= 0.6;
        }
        if self.bow_position != 0.5 {
            // For extreme bow positions, the volume should become quieter and more variable.

            let scale = 1.0 - 1.7*self.random.get_uniform()*(0.5-self.bow_position).abs();
            c *= scale*scale;
            if self.bow_position > 0.5 {
                // Without this, sul tasto gets a little too quiet.

                c *= 1.0+0.2*(self.bow_position-0.5);
            }
        }
        let volume = f32::min(1.0, self.volume);
        match &self.last_articulation {
            Articulation::Pizzicato | Articulation::ColLegno => {
                let x = (self.last_note-self.instrument_type.lowest_note()) as f32 / (self.instrument_type.highest_note()-self.instrument_type.lowest_note()) as f32;
                let decay_target;
                if x > 0.5 {
                    decay_target = (1.0-volume)*(1.5-x);
                }
                else {
                    decay_target = 1.0-volume;
                }
                for i in 1..self.spectrum_size {
                    let decay = 1.0-decay_target*(i as f32/self.spectrum_size as f32);
                    let scale = c*decay*(1.0-i as f32/self.spectrum_size as f32).powi(self.pizzicato_exponent);
                    self.spectrum_buffer[i] += Complex::<f32>::new(scale*self.random.get_uniform(), scale*self.random.get_uniform());
                }
            }
            _ => {
                // This is an empirical spectrum for the bow excitation, based on coefficients
                // determined from fitting to samples.

                let (m1, b1, m2, b2) = self.spectrum_coeff;
                let decay_target = 0.7*(1.0-volume)*(1.0-volume);
                for i in 1..self.spectrum_size {
                    let x = i as f32/self.spectrum_size as f32;
                    let logx = x.ln();
                    let y1 = f32::exp(-m1*logx + b1);
                    let y2 = f32::exp(-m2*logx + b2);
                    let mut decay;
                    if x <= 0.2 {
                        decay = 1.0-decay_target*x/0.2
                    }
                    else {
                        decay = 1.0-decay_target*(1.0-x)/0.8;
                    }
                    if self.bow_position > 0.5 {
                        // For sul tasto, reduce the high frequencies.

                        decay *= (-18.0*x*(self.bow_position-0.5)).exp();
                    }
                    else if self.bow_position < 0.5 {
                        // For sul ponticello, increase the high frequencies and reduce the fundamental.

                        decay *= (10.0*(x-0.5)*(x-0.5)*(0.5-self.bow_position)).exp();
                        if i == 1 {
                            decay *= 0.05+1.9*self.bow_position;
                        }
                    }
                    let scale = c*decay*f32::min(y1, y2);
                    self.spectrum_buffer[i] += Complex::<f32>::new(scale*self.random.get_uniform(), scale*self.random.get_uniform());
                }
            }
        }
    }

    /// Apply the filter to the spectrum buffer to damp the sound.
    fn apply_filter(&mut self) {
        if self.phase_shift != 0.0 {
            // Gradually shifting the phases of high frequencies helps to reduce artifacts.
            // I'm not entirely sure why!  I think it's related to the body resonance.

            for i in 1..self.spectrum_size {
                let phase = self.phase_shift*i as f32/self.spectrum_size as f32;
                self.spectrum_buffer[i] *= Complex::<f32>::new(phase.cos(), phase.sin());
            }
        }
        let s = if self.volume == 0.0 {0.2} else {0.15}; // Make the sound decay faster after the end of the note.
        for i in 1..self.spectrum_size {
            let f = i as f32/self.spectrum_size as f32;
            let scale = 1.0-(0.07-0.06*(-8.0*f).exp())*(self.spectrum_size as f32).sqrt()*s;
            self.spectrum_buffer[i] *= scale;
        }
        if self.harmonics {
            for i in 1..self.spectrum_size {
                if i%4 != 0 {
                    self.spectrum_buffer[i] *= 0.1;
                }
            }
        }
    }

    /// Generate the next audio sample.
    pub fn generate(&mut self, fft_planner: &mut RealFftPlanner::<f32>) -> f32 {
        if self.sample_rate == SAMPLE_RATE as f32 {
            self.generate_internal(fft_planner)
        }
        else {
            0.5*(self.generate_internal(fft_planner)+self.generate_internal(fft_planner))
        }
    }

    /// This is where the actual audio generation happens.  Depending on the instrument,
    /// audio generation can happen at either the output sample rate or twice that.  This
    /// generates data at the internal rate, and generate() calls it either once or twice
    /// for each output sample.
    fn generate_internal(&mut self, fft_planner: &mut RealFftPlanner::<f32>) -> f32 {
        let mut result = 0.0;
        if self.output_position >= self.output_size {
            if self.start_new_note {
                // We're at the start of a new note.  Move the tail of the previous note into
                // a separate object where it will be unaffected by further changes.

                if self.spectrum_size > 0 {
                    self.decaying_notes.push(DecayingNote::new(&self.spectrum_buffer[..self.spectrum_size], self.output_size));
                }
                for i in 1..self.spectrum_size {
                    self.spectrum_buffer[i] = Complex::<f32>::new(0.0, 0.0);
                }
                self.start_new_note = false;
            }

            // Add the sound from the tails of previous notes.

            for note in &mut self.decaying_notes {
                result += note.generate(fft_planner);
            }
            self.decaying_notes.retain(|n| !n.finished);

            // Vary the vibrato frequency and amplitude to give a more natural sound.

            self.vibrato_amplitude_drift = 0.99*self.vibrato_amplitude_drift + 0.1*self.random.get_normal();
            let x = (self.last_note-self.instrument_type.lowest_note()) as f32 / (self.instrument_type.highest_note()-self.instrument_type.lowest_note()) as f32;
            let vibrato_base_freq = self.vibrato_low_frequency + x*x*(self.vibrato_high_frequency-self.vibrato_low_frequency) + 0.5*self.vibrato_amplitude;
            let vibrato_freq = vibrato_base_freq * (1.0+self.vibrato_frequency_drift_amplitude*(0.5*PI*self.vibrato_phase).cos());
            let vibrato_amplitude;
            if self.last_note == self.instrument_type.lowest_note() {
                // Vibrato is impossible on an instrument's lowest note.

                vibrato_amplitude = 0.0;
            }
            else {
                vibrato_amplitude = self.vibrato_amplitude * (1.0+self.vibrato_amplitude_drift_amplitude*self.vibrato_amplitude_drift);
            }

            // Compute the instantaneous frequency.  This depends on the primary frequency of the note, vibrato, and random drift.

            let freq_drift_decay = (-self.period/self.sample_rate).exp();
            let freq_drift_noise = (1.0-freq_drift_decay*freq_drift_decay).sqrt();
            self.frequency_drift = freq_drift_decay*self.frequency_drift + freq_drift_noise*self.random.get_normal();
            let vibrato_offset = vibrato_freq*self.period/self.sample_rate;
            self.vibrato_phase = (self.vibrato_phase+vibrato_offset) % 4.0;
            let vibrato = (2.0*PI*self.vibrato_phase).sin();
            let vibrato = vibrato*vibrato*vibrato;
            let current_frequency = self.frequency * (1.0+self.frequency_drift_amplitude*self.frequency_drift) * (1.0+vibrato_amplitude*vibrato);

            // Update the buffer sizes.

            let new_period = self.sample_rate/current_frequency;
            let new_output_size = (new_period+self.period_offset).floor() as usize;
            let new_spectrum_size = (new_output_size as f32/2.0 + 1.0).floor() as usize;
            if new_output_size > self.output_buffer.len() {
                self.output_buffer.resize(new_output_size, 0.0);
            }
            if new_spectrum_size > self.spectrum_buffer.len() {
                self.spectrum_buffer.resize(new_spectrum_size, Complex::<f32>::new(0.0, 0.0));
                self.spectrum_temp.resize(new_spectrum_size, Complex::<f32>::new(0.0, 0.0));
            }
            for i in self.spectrum_size..new_spectrum_size {
                self.spectrum_buffer[i] = Complex::<f32>::new(0.0, 0.0);
            }
            self.period = new_period;
            self.output_size = new_output_size;
            self.spectrum_size = new_spectrum_size;
            self.period_offset = new_period+self.period_offset-new_output_size as f32;

            // Update the spectrum.

            if self.volume != 0.0 {
                self.add_bow_excitation();
            }
            self.apply_filter();

            // Generate a new batch of output.

            let fft = fft_planner.plan_fft_inverse(self.output_size);
            if self.scratch.len() < fft.get_scratch_len() {
                self.scratch.resize(fft.get_scratch_len(), Complex::<f32>::new(0.0, 0.0));
            }
            transform_spectrum(&fft, &self.spectrum_buffer[..self.spectrum_size], &mut self.spectrum_temp[..self.spectrum_size],
                               &mut self.output_buffer[..self.output_size], &mut self.scratch[..]);
            self.output_position = 0;
        }
        else {
            // Add the sound from the tails of previous notes.

            for note in &mut self.decaying_notes {
                result += note.generate(fft_planner);
            }
        }

        // Return output from the buffer.

        result += self.output_buffer[self.output_position];
        result = self.lowpass.process(result);
        self.output_position += 1;
        return result;
    }
}

struct DecayingNote {
    spectrum_buffer: Vec<Complex<f32>>,
    spectrum_temp: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    output_buffer: Vec<f32>,
    output_position: usize,
    finished: bool
}

impl DecayingNote {
    fn new(initial_spectrum: &[Complex<f32>], output_size: usize) -> Self {
        let mut result = Self {
            spectrum_buffer: vec![],
            spectrum_temp: vec![Complex::<f32>::new(0.0, 0.0); initial_spectrum.len()],
            scratch: vec![],
            output_buffer: vec![0.0; output_size],
            output_position: output_size,
            finished: false
        };
        result.spectrum_buffer.extend_from_slice(initial_spectrum);
        result
    }

    fn apply_filter(&mut self) {
        let spectrum_size = self.spectrum_buffer.len();
        for i in 1..spectrum_size {
            let f = i as f32/spectrum_size as f32;
            let scale = 1.0-(0.07-0.06*(-8.0*f).exp())*(spectrum_size as f32).sqrt()*0.15;
            self.spectrum_buffer[i] *= scale;
        }
    }

    fn generate(&mut self, fft_planner: &mut RealFftPlanner::<f32>) -> f32 {
        if self.finished {
            return 0.0;
        }
        let output_size = self.output_buffer.len();
        if self.output_position >= output_size {
            self.apply_filter();
            let fft = fft_planner.plan_fft_inverse(output_size);
            if self.scratch.len() < fft.get_scratch_len() {
                self.scratch.resize(fft.get_scratch_len(), Complex::<f32>::new(0.0, 0.0));
            }
            transform_spectrum(&fft, &self.spectrum_buffer[..], &mut self.spectrum_temp[..], &mut self.output_buffer[..], &mut self.scratch[..]);
            self.output_position = 0;
            self.finished = true;
            let mut max = 0.0;
            for i in 0..self.output_buffer.len() {
                if self.output_buffer[i].abs() > max {
                    max = self.output_buffer[i].abs();
                }
                if self.output_buffer[i].abs() > 0.001 {
                    self.finished = false;
                }
            }
        }
        let result = self.output_buffer[self.output_position];
        self.output_position += 1;
        return result;
    }
}

fn transform_spectrum(fft: &Arc<dyn ComplexToReal<f32>>, spectrum_buffer: &[Complex<f32>], spectrum_temp: &mut [Complex<f32>],
                      output_buffer: &mut [f32], scratch: &mut [Complex<f32>]) {
    spectrum_temp.copy_from_slice(&spectrum_buffer);
    if output_buffer.len()%2 == 0 {
        spectrum_temp[spectrum_temp.len()-1].im = 0.0;
    }
    match fft.process_with_scratch(spectrum_temp, output_buffer, scratch) {
        Ok(_) => {}
        Err(message) => {println!["{}", message]}
    }
}
