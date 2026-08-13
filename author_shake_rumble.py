#!/usr/bin/env python3
"""
author_shake_rumble.py - Synthesizes a looping sand shake/rumble sound effect.
Generates 16-bit 44.1kHz WAV file in assets/sfx/.
"""

import math
import os
import wave
import array
import random

SAMPLE_RATE = 44100
DURATION = 2.0  # 2 seconds looping

def generate_rumble():
    num_samples = int(SAMPLE_RATE * DURATION)
    samples = [0.0] * num_samples
    
    # Generate pink-ish noise with low-frequency rumble and gritty sand texture
    rng = random.Random(42)
    
    # Filter state
    lp_freq = 400.0
    lp_alpha = 2.0 * math.pi * lp_freq / SAMPLE_RATE
    lp_val = 0.0
    
    hp_freq = 40.0
    hp_alpha = 2.0 * math.pi * hp_freq / SAMPLE_RATE
    hp_val = 0.0
    
    for i in range(num_samples):
        # Base noise
        white = rng.uniform(-1.0, 1.0)
        
        # Amplitude modulation for the "shifting/tumbling" feel
        t = i / SAMPLE_RATE
        am1 = math.sin(2.0 * math.pi * 12.0 * t) * 0.5 + 0.5
        am2 = math.sin(2.0 * math.pi * 5.5 * t) * 0.5 + 0.5
        am3 = math.sin(2.0 * math.pi * 2.1 * t) * 0.5 + 0.5
        
        texture = white * (0.3 + 0.3 * am1 + 0.2 * am2 + 0.2 * am3)
        
        # Low pass
        lp_val += lp_alpha * (texture - lp_val)
        
        # High pass (remove DC / extreme lows)
        hp_val += hp_alpha * (lp_val - hp_val)
        out = lp_val - hp_val
        
        # Add some grit (higher frequency noise)
        grit_lp = rng.uniform(-1.0, 1.0) * 0.1
        
        samples[i] = out * 2.5 + grit_lp
        
    # Seamless loop crossfade
    crossfade_len = int(SAMPLE_RATE * 0.1) # 100ms
    for i in range(crossfade_len):
        fade_in = i / crossfade_len
        fade_out = 1.0 - fade_in
        # blend end to beginning
        start_val = samples[i]
        end_val = samples[num_samples - crossfade_len + i]
        blended = start_val * fade_in + end_val * fade_out
        samples[i] = blended
        samples[num_samples - crossfade_len + i] = blended

    # Normalize samples
    max_val = max(abs(s) for s in samples) or 1.0
    target_peak = 0.8
    norm_factor = target_peak / max_val
    
    int_samples = array.array('h')
    for s in samples:
        scaled = int(s * norm_factor * 32767.0)
        clamped = max(-32768, min(32767, scaled))
        int_samples.append(clamped)
        
    return int_samples

def main():
    os.makedirs("assets/sfx", exist_ok=True)
    path = os.path.join("assets/sfx", "shake_rumble.wav")
    pcm_data = generate_rumble()
    with wave.open(path, 'wb') as wf:
        wf.setnchannels(1) # Mono
        wf.setsampwidth(2) # 16-bit
        wf.setframerate(SAMPLE_RATE)
        wf.writeframes(pcm_data.tobytes())
    print(f"Generated {path} ({len(pcm_data)} samples)")

if __name__ == "__main__":
    main()
