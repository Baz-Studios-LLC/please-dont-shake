#!/usr/bin/env python3
"""
author_plastic_tap.py - Synthesizes realistic plastic tap sound effects for "Please Don't Shake".
Generates 16-bit 44.1kHz WAV files in assets/sfx/.
"""

import math
import os
import wave
import array
import random

SAMPLE_RATE = 44100

def generate_plastic_tap(freq_base, duration=0.15, skin_softness=0.5):
    num_samples = int(SAMPLE_RATE * duration)
    samples = [0.0] * num_samples
    
    # Plastic is heavily damped compared to glass, lower resonance and faster decay
    modes = [
        (1.0, 1.0, 0.015),    
        (1.6, 0.4, 0.010),
        (2.4, 0.15, 0.005),
    ]
    
    rng = random.Random(int(freq_base * 100))
    
    # 1. Plastic resonance
    for mode_ratio, amp, decay_t in modes:
        freq = freq_base * mode_ratio
        if freq >= SAMPLE_RATE * 0.45:
            continue
        decay_rate = 1.0 / decay_t
        phase_offset = rng.uniform(0, 2 * math.pi)
        
        for i in range(num_samples):
            t = i / SAMPLE_RATE
            env = math.exp(-t * decay_rate)
            pitch_mod = 1.0 + 0.02 * math.exp(-t * 200.0) # slight pitch drop
            sig = math.sin(2.0 * math.pi * freq * pitch_mod * t + phase_offset)
            samples[i] += sig * env * amp

    # 2. Fingertip impact transient (thud / click)
    impact_dur = int(SAMPLE_RATE * 0.01) # 10 ms impact
    for i in range(min(num_samples, impact_dur)):
        t = i / SAMPLE_RATE
        # Soft skin impact (low frequency pulse) + tiny click
        skin_thud = math.sin(2.0 * math.pi * 150.0 * t) * math.exp(-t * 800.0) * skin_softness
        click = (rng.uniform(-1.0, 1.0)) * math.exp(-t * 1500.0) * (1.0 - skin_softness * 0.5)
        samples[i] += (skin_thud + click * 0.3) * 1.5

    # Normalize samples
    max_val = max(abs(s) for s in samples) or 1.0
    target_peak = 0.85
    norm_factor = target_peak / max_val
    
    int_samples = array.array('h')
    for s in samples:
        scaled = int(s * norm_factor * 32767.0)
        clamped = max(-32768, min(32767, scaled))
        int_samples.append(clamped)
        
    return int_samples

def main():
    os.makedirs("assets/sfx", exist_ok=True)
    
    # Frequencies lower than glass (which were ~2000Hz)
    taps = [
        ("plastic_tap_1.wav", 400.0, 0.15, 0.4),
        ("plastic_tap_2.wav", 450.0, 0.15, 0.35),
        ("plastic_tap_3.wav", 350.0, 0.15, 0.5),
    ]
    
    for filename, freq, dur, softness in taps:
        path = os.path.join("assets/sfx", filename)
        pcm_data = generate_plastic_tap(freq, duration=dur, skin_softness=softness)
        with wave.open(path, 'wb') as wf:
            wf.setnchannels(1) # Mono
            wf.setsampwidth(2) # 16-bit
            wf.setframerate(SAMPLE_RATE)
            wf.writeframes(pcm_data.tobytes())
        print(f"Generated {path} ({len(pcm_data)} samples)")

if __name__ == "__main__":
    main()
