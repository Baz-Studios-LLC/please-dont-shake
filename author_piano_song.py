#!/usr/bin/env python3
"""
author_piano_song.py - Instant piano music authoring script with quantized note caching.
Generates a full-length (~3.1 minute), seamless-looping classical piano WAV for "Please Don't Shake".
"""

import math
import wave
import os
import array
import time

t0 = time.time()

SAMPLE_RATE = 44100
BPM = 62.0
BEAT_DUR = 60.0 / BPM  # ~0.9677 seconds per quarter note
MEASURE_DUR = BEAT_DUR * 3.0  # 3/4 time signature (~2.9032 seconds per bar)

TABLE_BITS = 16
TABLE_SIZE = 1 << TABLE_BITS
SINE_TABLE = array.array('f', [math.sin(2.0 * math.pi * i / TABLE_SIZE) for i in range(TABLE_SIZE)])

def midi_to_freq(note):
    return 440.0 * (2.0 ** ((note - 69) / 12.0))

def render_note_waveform(note, dur_sec, vel):
    freq = midi_to_freq(note)
    
    h_data = []
    base_decay = 0.7 + 160.0 / (freq + 60.0)
    for h in range(1, 5):
        freq_h = h * freq * math.sqrt(1.0 + 0.0003 * h * h)
        if freq_h >= SAMPLE_RATE * 0.45:
            break
        amp_h = (1.0 / (h ** 1.15)) * (0.4 + 0.6 * vel)
        decay_h = (2.8 / base_decay) * (h ** 0.5)
        
        step_idx = int((freq_h / SAMPLE_RATE) * TABLE_SIZE)
        decay_mult = math.exp(-decay_h / SAMPLE_RATE)
        h_data.append((step_idx, amp_h, decay_mult))
        
    note_samples = int((dur_sec + 3.0) * SAMPLE_RATE)
    out = array.array('f', [0.0] * note_samples)
    
    for step_idx, amp_h, decay_mult in h_data:
        idx = 0
        env = amp_h
        for s in range(note_samples):
            if env < 0.0001:
                break
            out[s] += env * SINE_TABLE[idx & (TABLE_SIZE - 1)]
            idx += step_idx
            env *= decay_mult
            
    rel_start = int(dur_sec * SAMPLE_RATE)
    if rel_start < note_samples:
        rel_mult = math.exp(-10.0 / SAMPLE_RATE)
        rel_env = 1.0
        for s in range(rel_start, note_samples):
            out[s] *= rel_env
            rel_env *= rel_mult
            
    return out

def render_piano_track(events, total_measures):
    song_duration = total_measures * MEASURE_DUR
    total_samples = int(song_duration * SAMPLE_RATE)
    tail_samples = int(4.0 * SAMPLE_RATE)
    buffer_samples = total_samples + tail_samples
    
    left_buf = array.array('f', [0.0] * buffer_samples)
    right_buf = array.array('f', [0.0] * buffer_samples)
    
    print(f"Synthesizing {len(events)} note events across {total_measures} bars ({song_duration:.1f}s)...")
    
    note_cache = {}
    
    for event in events:
        start_beat, note, dur_beats, vel = event
        start_sample = int(start_beat * BEAT_DUR * SAMPLE_RATE)
        dur_sec = dur_beats * BEAT_DUR
        
        # Quantize cache key so notes share pre-rendered wave tables
        q_dur = round(dur_sec * 2.0) / 2.0  # round to 0.5s intervals
        q_vel = round(vel * 4.0) / 4.0      # round to 0.25 velocity steps
        
        cache_key = (note, q_dur, q_vel)
        if cache_key not in note_cache:
            note_cache[cache_key] = render_note_waveform(note, q_dur, q_vel)
            
        note_wave = note_cache[cache_key]
        
        pan = max(-0.35, min(0.35, (note - 60) / 48.0))
        gl = (0.5 - pan * 0.5) * vel
        gr = (0.5 + pan * 0.5) * vel
        
        count = min(len(note_wave), buffer_samples - start_sample)
        for s in range(count):
            samp = start_sample + s
            v = note_wave[s]
            left_buf[samp] += v * gl
            right_buf[samp] += v * gr
            
    print(f"Notes synthesized using {len(note_cache)} cached wave templates.")
    
    reverb_delay = int(0.038 * SAMPLE_RATE)
    rev_gain = 0.28
    for i in range(reverb_delay, buffer_samples):
        left_buf[i] += left_buf[i - reverb_delay] * rev_gain
        right_buf[i] += right_buf[i - reverb_delay] * rev_gain
        
    print("Performing seamless loop wrapping...")
    for i in range(tail_samples):
        left_buf[i] += left_buf[total_samples + i]
        right_buf[i] += right_buf[total_samples + i]
        
    max_amp = 0.00001
    for i in range(total_samples):
        max_amp = max(max_amp, abs(left_buf[i]), abs(right_buf[i]))
        
    norm_factor = 0.82 / max_amp
    print(f"Normalizing audio (peak was {max_amp:.3f}, gain factor {norm_factor:.3f})...")
    
    target_file = os.path.join("assets", "music", "cozy_piano.wav")
    os.makedirs(os.path.dirname(target_file), exist_ok=True)
    
    print(f"Writing WAV audio file {target_file}...")
    pcm = array.array('h', [0] * (total_samples * 2))
    
    for i in range(total_samples):
        l_val = max(-32768, min(32767, int(left_buf[i] * norm_factor * 32767.0)))
        r_val = max(-32768, min(32767, int(right_buf[i] * norm_factor * 32767.0)))
        pcm[i * 2] = l_val
        pcm[i * 2 + 1] = r_val
        
    with wave.open(target_file, 'wb') as wav_file:
        wav_file.setnchannels(2)
        wav_file.setsampwidth(2)
        wav_file.setframerate(SAMPLE_RATE)
        wav_file.writeframes(pcm.tobytes())
        
    t1 = time.time()
    print(f"WAV file {target_file} created successfully in {t1 - t0:.2f}s ({os.path.getsize(target_file)} bytes)!")

class PianoComposer:
    def __init__(self):
        self.events = []

    def note(self, beat, pitch, dur, vel=0.5):
        self.events.append((beat, pitch, dur, vel))

    def chord(self, beat, pitches, dur, vel=0.5, arpeggiate_speed=0.0):
        for i, pitch in enumerate(pitches):
            offset = i * arpeggiate_speed
            self.note(beat + offset, pitch, dur, vel * (0.95 ** i))

def compose_full_song():
    c = PianoComposer()
    
    def b(bar, beat_in_bar=1.0):
        return (bar - 1) * 3.0 + (beat_in_bar - 1.0)
        
    # INTRO (Bars 1-8): "First Light in the Tank"
    c.note(b(1, 1), 36, 4.0, 0.45) # C2 bass
    c.chord(b(1, 2), [48, 55, 59, 62], 2.0, 0.35, 0.08) # C3, G3, B3, D4
    c.note(b(1, 3.5), 67, 1.5, 0.40) # G4
    
    c.note(b(2, 1), 48, 2.5, 0.35)
    c.chord(b(2, 2), [55, 59, 62, 64], 2.0, 0.35, 0.06)
    c.note(b(2, 2.5), 71, 1.5, 0.42)
    c.note(b(2, 3.5), 72, 1.5, 0.45)
    
    c.note(b(3, 1), 41, 4.0, 0.45)
    c.chord(b(3, 2), [53, 57, 60, 64], 2.0, 0.38, 0.08)
    c.note(b(3, 2), 76, 2.0, 0.46)
    c.note(b(3, 3), 74, 1.5, 0.42)
    
    c.note(b(4, 1), 53, 2.5, 0.35)
    c.chord(b(4, 2), [57, 60, 64, 67], 2.0, 0.35, 0.06)
    c.note(b(4, 2.5), 72, 1.5, 0.44)
    c.note(b(4, 3.5), 71, 1.5, 0.40)
    
    c.note(b(5, 1), 45, 4.0, 0.42)
    c.chord(b(5, 2), [52, 55, 60, 64], 2.0, 0.36, 0.08)
    c.note(b(5, 1.5), 69, 2.0, 0.42)
    c.note(b(5, 3), 67, 1.5, 0.38)
    
    c.note(b(6, 1), 38, 4.0, 0.42)
    c.chord(b(6, 2), [50, 54, 57, 62], 2.0, 0.36, 0.08)
    c.note(b(6, 2), 66, 1.5, 0.40)
    c.note(b(6, 3), 67, 1.5, 0.42)
    
    c.note(b(7, 1), 38, 4.0, 0.40)
    c.chord(b(7, 2), [50, 53, 57, 60], 2.0, 0.35, 0.07)
    c.note(b(7, 1.5), 69, 2.0, 0.42)
    c.note(b(7, 3), 71, 1.5, 0.45)
    
    c.note(b(8, 1), 43, 4.0, 0.42)
    c.chord(b(8, 2), [50, 55, 60, 62], 2.5, 0.38, 0.08)
    c.note(b(8, 2), 72, 2.5, 0.46)
    c.note(b(8, 3.5), 71, 1.5, 0.40)
    
    # SECTION A (Bars 9-24): "Ant Hill Lullaby"
    c.note(b(9, 1), 36, 4.0, 0.55)
    c.chord(b(9, 2), [48, 55, 59, 64], 2.0, 0.40, 0.06)
    c.chord(b(9, 3), [55, 59, 64], 1.5, 0.38)
    c.note(b(9, 1), 76, 2.0, 0.55)
    c.note(b(9, 2.5), 74, 1.0, 0.48)
    c.note(b(9, 3.5), 72, 1.5, 0.50)
    
    c.note(b(10, 1), 45, 4.0, 0.50)
    c.chord(b(10, 2), [52, 55, 60, 64], 2.0, 0.40, 0.06)
    c.chord(b(10, 3), [55, 60, 64], 1.5, 0.38)
    c.note(b(10, 1), 71, 2.0, 0.52)
    c.note(b(10, 2.5), 69, 1.0, 0.46)
    c.note(b(10, 3.5), 67, 1.5, 0.48)
    
    c.note(b(11, 1), 41, 4.0, 0.52)
    c.chord(b(11, 2), [53, 57, 60, 64, 67], 2.0, 0.42, 0.07)
    c.chord(b(11, 3), [57, 60, 64], 1.5, 0.38)
    c.note(b(11, 1), 69, 2.5, 0.54)
    c.note(b(11, 3), 72, 1.5, 0.52)
    
    c.note(b(12, 1), 40, 4.0, 0.48)
    c.chord(b(12, 2), [52, 55, 59, 62], 2.0, 0.38, 0.06)
    c.note(b(12, 1), 71, 2.5, 0.50)
    c.note(b(12, 3), 67, 1.5, 0.45)
    
    c.note(b(13, 1), 38, 4.0, 0.50)
    c.chord(b(13, 2), [50, 57, 60, 65], 2.0, 0.40, 0.06)
    c.chord(b(13, 3), [57, 60, 65], 1.5, 0.38)
    c.note(b(13, 1), 74, 2.0, 0.54)
    c.note(b(13, 2.5), 76, 1.0, 0.50)
    c.note(b(13, 3.5), 77, 1.5, 0.55)
    
    c.note(b(14, 1), 43, 4.0, 0.52)
    c.chord(b(14, 2), [50, 53, 57, 64], 2.0, 0.40, 0.06)
    c.note(b(14, 1), 79, 2.0, 0.58)
    c.note(b(14, 2.5), 77, 1.0, 0.52)
    c.note(b(14, 3.5), 76, 1.5, 0.50)
    
    c.note(b(15, 1), 36, 4.0, 0.54)
    c.chord(b(15, 2), [48, 55, 59, 64], 2.0, 0.40, 0.06)
    c.note(b(15, 1), 74, 2.0, 0.52)
    c.note(b(15, 2.5), 72, 1.0, 0.48)
    c.note(b(15, 3.5), 71, 1.5, 0.46)
    
    c.note(b(16, 1), 48, 2.5, 0.42)
    c.chord(b(16, 2), [55, 58, 64, 67], 2.0, 0.40, 0.06)
    c.note(b(16, 1), 69, 2.5, 0.48)
    c.note(b(16, 3), 67, 1.5, 0.45)
    
    c.note(b(17, 1), 41, 4.0, 0.55)
    c.chord(b(17, 2), [53, 57, 60, 64], 2.0, 0.42, 0.06)
    c.note(b(17, 1), 81, 2.0, 0.58)
    c.note(b(17, 2.5), 79, 1.0, 0.52)
    c.note(b(17, 3.5), 76, 1.5, 0.50)
    
    c.note(b(18, 1), 41, 4.0, 0.52)
    c.chord(b(18, 2), [53, 56, 62, 65], 2.0, 0.42, 0.06)
    c.note(b(18, 1), 77, 2.0, 0.54)
    c.note(b(18, 2.5), 76, 1.0, 0.48)
    c.note(b(18, 3.5), 74, 1.5, 0.48)
    
    c.note(b(19, 1), 40, 4.0, 0.50)
    c.chord(b(19, 2), [52, 55, 59, 64], 2.0, 0.40, 0.06)
    c.note(b(19, 1), 76, 2.0, 0.52)
    c.note(b(19, 2.5), 72, 1.0, 0.46)
    c.note(b(19, 3.5), 71, 1.5, 0.45)
    
    c.note(b(20, 1), 45, 4.0, 0.48)
    c.chord(b(20, 2), [52, 55, 58, 61, 64], 2.0, 0.40, 0.06)
    c.note(b(20, 1), 69, 2.5, 0.50)
    c.note(b(20, 3), 67, 1.5, 0.45)
    
    c.note(b(21, 1), 38, 4.0, 0.52)
    c.chord(b(21, 2), [50, 57, 60, 64, 69], 2.0, 0.42, 0.06)
    c.note(b(21, 1), 74, 2.0, 0.54)
    c.note(b(21, 2.5), 72, 1.0, 0.48)
    c.note(b(21, 3.5), 69, 1.5, 0.46)
    
    c.note(b(22, 1), 43, 4.0, 0.50)
    c.chord(b(22, 2), [50, 57, 60, 65], 2.0, 0.40, 0.06)
    c.note(b(22, 1), 71, 2.5, 0.50)
    c.note(b(22, 3), 67, 1.5, 0.46)
    
    c.note(b(23, 1), 36, 4.0, 0.50)
    c.chord(b(23, 2), [48, 55, 59, 64], 2.0, 0.38, 0.06)
    c.note(b(23, 1), 64, 3.0, 0.48)
    
    c.note(b(24, 1), 43, 4.0, 0.46)
    c.chord(b(24, 2), [50, 55, 60, 62], 2.0, 0.36, 0.06)
    c.note(b(24, 2), 65, 1.5, 0.42)
    c.note(b(24, 3), 67, 1.5, 0.44)
    
    # SECTION B (Bars 25-40): "Chambers in the Earth"
    c.note(b(25, 1), 33, 4.0, 0.52)
    c.chord(b(25, 2), [45, 52, 55, 60, 64], 2.0, 0.40, 0.08)
    c.note(b(25, 1), 69, 1.5, 0.52)
    c.note(b(25, 2.5), 71, 1.0, 0.48)
    c.note(b(25, 3.5), 72, 1.5, 0.50)
    
    c.note(b(26, 1), 43, 4.0, 0.48)
    c.chord(b(26, 2), [52, 55, 60, 64], 2.0, 0.38, 0.06)
    c.note(b(26, 1), 74, 2.0, 0.54)
    c.note(b(26, 2.5), 72, 1.0, 0.48)
    c.note(b(26, 3.5), 71, 1.5, 0.46)
    
    c.note(b(27, 1), 42, 4.0, 0.50)
    c.chord(b(27, 2), [54, 57, 60, 64], 2.0, 0.40, 0.06)
    c.note(b(27, 1), 69, 2.5, 0.50)
    c.note(b(27, 3), 72, 1.5, 0.52)
    
    c.note(b(28, 1), 41, 4.0, 0.52)
    c.chord(b(28, 2), [53, 57, 60, 64], 2.0, 0.40, 0.06)
    c.note(b(28, 1), 76, 2.0, 0.56)
    c.note(b(28, 2.5), 74, 1.0, 0.50)
    c.note(b(28, 3.5), 72, 1.5, 0.48)
    
    c.note(b(29, 1), 40, 4.0, 0.48)
    c.chord(b(29, 2), [52, 55, 59, 62], 2.0, 0.38, 0.06)
    c.note(b(29, 1), 71, 2.5, 0.50)
    c.note(b(29, 3), 67, 1.5, 0.45)
    
    c.note(b(30, 1), 45, 4.0, 0.50)
    c.chord(b(30, 2), [52, 55, 61, 64], 2.0, 0.40, 0.06)
    c.note(b(30, 1), 69, 2.0, 0.50)
    c.note(b(30, 2.5), 71, 1.0, 0.48)
    c.note(b(30, 3.5), 73, 1.5, 0.52)
    
    c.note(b(31, 1), 38, 4.0, 0.52)
    c.chord(b(31, 2), [50, 57, 60, 64, 69], 2.0, 0.40, 0.06)
    c.note(b(31, 1), 74, 2.0, 0.54)
    c.note(b(31, 2.5), 76, 1.0, 0.50)
    c.note(b(31, 3.5), 77, 1.5, 0.52)
    
    c.note(b(32, 1), 43, 4.0, 0.50)
    c.chord(b(32, 2), [50, 55, 60, 62], 2.0, 0.38, 0.06)
    c.note(b(32, 1), 79, 2.0, 0.56)
    c.note(b(32, 2.5), 77, 1.0, 0.50)
    c.note(b(32, 3.5), 76, 1.5, 0.48)
    
    c.note(b(33, 1), 36, 4.0, 0.54)
    c.chord(b(33, 2), [48, 55, 59, 64], 2.0, 0.40, 0.06)
    c.note(b(33, 1), 76, 2.0, 0.54)
    c.note(b(33, 2.5), 74, 1.0, 0.48)
    c.note(b(33, 3.5), 72, 1.5, 0.46)
    
    c.note(b(34, 1), 46, 4.0, 0.48)
    c.chord(b(34, 2), [52, 55, 60, 64], 2.0, 0.38, 0.06)
    c.note(b(34, 1), 70, 2.5, 0.48)
    c.note(b(34, 3), 67, 1.5, 0.44)
    
    c.note(b(35, 1), 45, 4.0, 0.52)
    c.chord(b(35, 2), [53, 57, 60, 64], 2.0, 0.40, 0.06)
    c.note(b(35, 1), 69, 2.0, 0.52)
    c.note(b(35, 2.5), 72, 1.0, 0.50)
    c.note(b(35, 3.5), 76, 1.5, 0.54)
    
    c.note(b(36, 1), 44, 4.0, 0.50)
    c.chord(b(36, 2), [53, 56, 62, 65], 2.0, 0.38, 0.06)
    c.note(b(36, 1), 77, 2.0, 0.54)
    c.note(b(36, 2.5), 76, 1.0, 0.48)
    c.note(b(36, 3.5), 74, 1.5, 0.46)
    
    c.note(b(37, 1), 43, 4.0, 0.50)
    c.chord(b(37, 2), [48, 52, 55, 60, 64], 2.0, 0.38, 0.06)
    c.note(b(37, 1), 72, 3.0, 0.50)
    
    c.note(b(38, 1), 42, 4.0, 0.48)
    c.chord(b(38, 2), [50, 54, 57, 62, 64], 2.0, 0.38, 0.06)
    c.note(b(38, 1), 66, 1.5, 0.44)
    c.note(b(38, 2.5), 69, 1.5, 0.46)
    
    c.note(b(39, 1), 43, 4.0, 0.48)
    c.chord(b(39, 2), [50, 57, 60, 65], 2.0, 0.38, 0.06)
    c.note(b(39, 1), 71, 2.5, 0.48)
    c.note(b(39, 3), 67, 1.5, 0.44)
    
    c.note(b(40, 1), 43, 4.0, 0.46)
    c.chord(b(40, 2), [50, 53, 59, 62], 2.0, 0.36, 0.06)
    c.note(b(40, 1), 65, 1.5, 0.42)
    c.note(b(40, 2.5), 67, 1.5, 0.44)
    
    # SECTION C (Bars 41-56): "Through the Glass"
    c.note(b(41, 1), 41, 4.0, 0.58)
    c.chord(b(41, 2), [53, 57, 60, 64], 2.0, 0.42, 0.06)
    c.note(b(41, 1), 76, 1.5, 0.58)
    c.note(b(41, 2.5), 77, 1.0, 0.54)
    c.note(b(41, 3.5), 79, 1.5, 0.56)
    
    c.note(b(42, 1), 41, 4.0, 0.54)
    c.chord(b(42, 2), [50, 55, 59, 62], 2.0, 0.40, 0.06)
    c.note(b(42, 1), 81, 2.0, 0.60)
    c.note(b(42, 2.5), 79, 1.0, 0.54)
    c.note(b(42, 3.5), 76, 1.5, 0.52)
    
    c.note(b(43, 1), 40, 4.0, 0.54)
    c.chord(b(43, 2), [52, 55, 59, 64], 2.0, 0.40, 0.06)
    c.note(b(43, 1), 79, 2.0, 0.58)
    c.note(b(43, 2.5), 76, 1.0, 0.52)
    c.note(b(43, 3.5), 74, 1.5, 0.50)
    
    c.note(b(44, 1), 45, 4.0, 0.52)
    c.chord(b(44, 2), [52, 55, 60, 64], 2.0, 0.38, 0.06)
    c.note(b(44, 1), 72, 2.5, 0.52)
    c.note(b(44, 3), 69, 1.5, 0.46)
    
    c.note(b(45, 1), 38, 4.0, 0.54)
    c.chord(b(45, 2), [50, 57, 60, 65], 2.0, 0.40, 0.06)
    c.note(b(45, 1), 77, 2.0, 0.56)
    c.note(b(45, 2.5), 76, 1.0, 0.50)
    c.note(b(45, 3.5), 74, 1.5, 0.48)
    
    c.note(b(46, 1), 43, 4.0, 0.52)
    c.chord(b(46, 2), [50, 53, 57, 62], 2.0, 0.38, 0.06)
    c.note(b(46, 1), 74, 2.0, 0.54)
    c.note(b(46, 2.5), 72, 1.0, 0.48)
    c.note(b(46, 3.5), 71, 1.5, 0.46)
    
    c.note(b(47, 1), 36, 4.0, 0.56)
    c.chord(b(47, 2), [48, 55, 59, 64], 2.0, 0.40, 0.06)
    c.note(b(47, 1), 72, 2.5, 0.52)
    c.note(b(47, 3), 76, 1.5, 0.54)
    
    c.note(b(48, 1), 48, 2.5, 0.45)
    c.chord(b(48, 2), [55, 58, 64, 67], 2.0, 0.38, 0.06)
    c.note(b(48, 1), 79, 2.0, 0.56)
    c.note(b(48, 2.5), 77, 1.0, 0.50)
    c.note(b(48, 3.5), 76, 1.5, 0.48)
    
    c.note(b(49, 1), 41, 4.0, 0.58)
    c.chord(b(49, 2), [53, 57, 60, 64, 66], 2.0, 0.42, 0.06)
    c.note(b(49, 1), 81, 2.0, 0.60)
    c.note(b(49, 2.5), 83, 1.0, 0.56)
    c.note(b(49, 3.5), 84, 1.5, 0.58)
    
    c.note(b(50, 1), 53, 2.5, 0.42)
    c.chord(b(50, 2), [57, 60, 64, 67], 2.0, 0.38, 0.06)
    c.note(b(50, 1), 81, 2.0, 0.56)
    c.note(b(50, 2.5), 79, 1.0, 0.50)
    c.note(b(50, 3.5), 76, 1.5, 0.48)
    
    c.note(b(51, 1), 40, 4.0, 0.52)
    c.chord(b(51, 2), [52, 55, 59, 64], 2.0, 0.38, 0.06)
    c.note(b(51, 1), 79, 2.0, 0.54)
    c.note(b(51, 2.5), 76, 1.0, 0.48)
    c.note(b(51, 3.5), 74, 1.5, 0.46)
    
    c.note(b(52, 1), 45, 4.0, 0.50)
    c.chord(b(52, 2), [52, 55, 61, 64], 2.0, 0.38, 0.06)
    c.note(b(52, 1), 73, 2.5, 0.50)
    c.note(b(52, 3), 69, 1.5, 0.45)
    
    c.note(b(53, 1), 38, 4.0, 0.52)
    c.chord(b(53, 2), [50, 57, 60, 64, 69], 2.0, 0.40, 0.06)
    c.note(b(53, 1), 77, 2.0, 0.54)
    c.note(b(53, 2.5), 76, 1.0, 0.48)
    c.note(b(53, 3.5), 74, 1.5, 0.46)
    
    c.note(b(54, 1), 43, 4.0, 0.50)
    c.chord(b(54, 2), [50, 57, 60, 65], 2.0, 0.38, 0.06)
    c.note(b(54, 1), 72, 2.5, 0.50)
    c.note(b(54, 3), 71, 1.5, 0.46)
    
    c.note(b(55, 1), 43, 4.0, 0.48)
    c.chord(b(55, 2), [50, 55, 60, 62], 2.0, 0.36, 0.06)
    c.note(b(55, 1), 69, 2.0, 0.46)
    c.note(b(55, 2.5), 71, 1.0, 0.44)
    c.note(b(55, 3.5), 72, 1.5, 0.46)
    
    c.note(b(56, 1), 43, 4.0, 0.46)
    c.chord(b(56, 2), [50, 53, 57, 62], 2.0, 0.35, 0.06)
    c.note(b(56, 1), 74, 2.5, 0.48)
    c.note(b(56, 3), 71, 1.5, 0.42)
    
    # SECTION A' / OUTRO (Bars 57-64): "Quiet Rebuilding"
    c.note(b(57, 1), 36, 4.0, 0.42)
    c.chord(b(57, 2), [48, 55, 59, 64], 2.0, 0.32, 0.06)
    c.note(b(57, 1), 76, 2.0, 0.44)
    c.note(b(57, 2.5), 74, 1.0, 0.38)
    c.note(b(57, 3.5), 72, 1.5, 0.36)
    
    c.note(b(58, 1), 45, 4.0, 0.40)
    c.chord(b(58, 2), [52, 55, 60, 64], 2.0, 0.30, 0.06)
    c.note(b(58, 1), 71, 2.5, 0.40)
    c.note(b(58, 3), 67, 1.5, 0.35)
    
    c.note(b(59, 1), 41, 4.0, 0.40)
    c.chord(b(59, 2), [53, 57, 60, 64], 2.0, 0.30, 0.06)
    c.note(b(59, 1), 69, 2.5, 0.40)
    c.note(b(59, 3), 72, 1.5, 0.38)
    
    c.note(b(60, 1), 38, 4.0, 0.38)
    c.chord(b(60, 2), [50, 57, 60, 64], 2.0, 0.28, 0.06)
    c.note(b(60, 1), 74, 2.5, 0.40)
    c.note(b(60, 3), 71, 1.5, 0.35)
    
    c.note(b(61, 1), 40, 4.0, 0.36)
    c.chord(b(61, 2), [52, 55, 59], 2.0, 0.26, 0.06)
    c.note(b(61, 1), 67, 3.0, 0.36)
    
    c.note(b(62, 1), 45, 4.0, 0.35)
    c.chord(b(62, 2), [52, 55, 60], 2.0, 0.25, 0.06)
    c.note(b(62, 1), 64, 3.0, 0.34)
    
    c.note(b(63, 1), 43, 4.0, 0.35)
    c.chord(b(63, 2), [50, 55, 60, 62], 2.0, 0.26, 0.06)
    c.note(b(63, 1.5), 67, 1.5, 0.34)
    c.note(b(63, 3), 71, 1.5, 0.36)
    
    c.note(b(64, 1), 43, 4.5, 0.32)
    c.chord(b(64, 2), [50, 55, 60, 62], 3.0, 0.24, 0.06)
    c.note(b(64, 2), 72, 3.0, 0.35)
    c.note(b(64, 3.5), 71, 1.5, 0.30)
    
    return c.events, 64

if __name__ == '__main__':
    events, total_bars = compose_full_song()
    render_piano_track(events, total_bars)
