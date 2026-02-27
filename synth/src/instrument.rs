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
use crate::InstrumentType;
use crate::SAMPLE_RATE;
use realfft::{RealFftPlanner, ComplexToReal};
use rustfft::num_complex::Complex;

/// This struct combines a glottal source and two waveguides to form the complete synthesis model.
/// In addition, consonants can be synthesized by injecting extra noise at an arbitrary point in
/// the vocal tract.
pub struct Instrument {
    volume: f32,
    frequency: f32,
    vibrato_frequency: f32,
    vibrato_amplitude: f32,
    tremolo_amplitude: f32,
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
    decaying_notes: Vec<DecayingNote>,
    start_new_note: bool,
    last_note: i32
}

impl Instrument {
    pub fn new(instrument_type: InstrumentType, index: usize) -> Self {
        let mut random = Random::new();
        Self {
            volume: 1.0,
            frequency: 440.0,
            vibrato_frequency: 1.0,
            vibrato_amplitude: 0.0,
            tremolo_amplitude: 0.0,
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
            decaying_notes: vec![],
            start_new_note: false,
            last_note: 0
        }
    }

    /// Signal the start of a new note.
    pub fn note_on(&mut self, note: i32) {
        if note != self.last_note {
            self.start_new_note = true;
            self.last_note = note;
        }
    }

    /// Get the volume of the excitation from the bow (between 0.0 and 1.0).
    pub fn get_volume(&self) -> f32 {
        self.volume
    }

    /// Set the volume of the excitation from the bow (between 0.0 and 1.0).
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
    }

    /// Set the frequency of the glottal excitation (in Hz).
    pub fn set_frequency(&mut self, frequency: f32) {
        self.frequency = frequency;
    }

    /// Set the amplitude of the bow noise.
    pub fn set_noise(&mut self, noise: f32) {
        // self.glottis.noise = noise;
    }

    /// Get the frequency of vibrato.
    pub fn get_vibrato_frequency(&self) -> f32 {
        self.vibrato_frequency
    }

    /// Set the frequency of vibrato.
    pub fn set_vibrato_frequency(&mut self, frequency: f32) {
        self.vibrato_frequency = frequency;
    }

    /// Set the amplitude of vibrato.
    pub fn set_vibrato_amplitude(&mut self, amplitude: f32) {
        self.vibrato_amplitude = amplitude;
    }

    /// Set the amplitude of tremolo.
    pub fn set_tremolo_amplitude(&mut self, amplitude: f32) {
        self.tremolo_amplitude = amplitude;
    }

    /// Add excitation from the bow to the spectrum.
    fn add_bow_excitation(&mut self) {
        let c = self.volume/(self.spectrum_size as f32).sqrt();
        for i in 1..self.spectrum_size {
           let scale = c*(1.0-(i as f32-3.0).abs()/self.spectrum_size as f32).powi(20);
           let c1 = self.random.get_uniform();
           let c2 = self.random.get_uniform();
           self.spectrum_buffer[i] += Complex::<f32>::new(scale*(1.0-c1*c1), scale*(1.0-c2*c2));
        }
    }

    /// Apply the filter to the spectrum buffer to damp the sound.
    fn apply_filter(&mut self) {
        for i in 1..self.spectrum_size {
            let f = i as f32/self.spectrum_size as f32;
            let scale = 1.0-(0.07-0.06*(-8.0*f).exp())*(self.spectrum_size as f32).sqrt()*0.1;
            self.spectrum_buffer[i] *= scale;
        }
    }

    /// Generate the next audio sample.
    pub fn generate(&mut self, step: i64, fft_planner: &mut RealFftPlanner::<f32>) -> f32 {
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

            // Compute the instantaneous frequency and update the buffer sizes.

            let current_frequency = self.frequency;
            let new_period = SAMPLE_RATE as f32/current_frequency;
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
            for note in &mut self.decaying_notes {
                result += note.generate(fft_planner);
            }
        }

        // Return output from the buffer.

        result += self.output_buffer[self.output_position];
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
            let scale = 1.0-(0.07-0.06*(-8.0*f).exp())*(spectrum_size as f32).sqrt()*0.1;
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
