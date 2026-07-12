import librosa
import json
import sys
import os
import numpy as np

def generate_chart(audio_path, output_path=None):
    if not output_path:
        base, _ = os.path.splitext(audio_path)
        output_path = base + ".json"

    print(f"Loading {audio_path}...")
    y, sr = librosa.load(audio_path, sr=None)

    print("Tracking beats...")
    # librosa.beat.beat_track returns: (tempo, beat_frames) by default, or beat_times if units='time'
    tempo, beat_times = librosa.beat.beat_track(y=y, sr=sr, units='time')
    
    # tempo can be an array/scalar depending on librosa version, handle safely
    if isinstance(tempo, (list, np.ndarray)):
        bpm = float(tempo[0])
    else:
        bpm = float(tempo)
        
    print(f"Detected BPM: {bpm:.2f}")
    
    # Detect onset strengths & times to place extra half-beat notes on strong beats/drums
    onset_env = librosa.onset.onset_strength(y=y, sr=sr)
    onset_frames = librosa.onset.onset_detect(onset_envelope=onset_env, sr=sr)
    onset_times = librosa.frames_to_time(onset_frames, sr=sr)
    
    candidates = list(beat_times)
    
    # 2. Gather midpoints if they have strong onsets
    for i in range(len(beat_times) - 1):
        t1 = beat_times[i]
        t2 = beat_times[i+1]
        mid = (t1 + t2) / 2.0
        for ot in onset_times:
            if abs(ot - mid) < 0.05:
                candidates.append(mid)
                break
                
    candidates = sorted(list(set(candidates)))

    # Compute onset strength at each candidate time
    candidate_frames = librosa.time_to_frames(candidates, sr=sr)
    candidate_frames = np.clip(candidate_frames, 0, len(onset_env) - 1)
    strengths = onset_env[candidate_frames]
    
    mean_strength = np.mean(strengths) if len(strengths) > 0 else 1.0
    
    final_times = []
    last_time = -999.0
    
    for idx, t in enumerate(candidates):
        strength = strengths[idx]
        is_strong = strength >= 1.0 * mean_strength
        
        # Enforce minimum time gap between notes for playability (0.55s)
        if t - last_time >= 0.55:
            # Keep if the onset is relatively strong, or if we haven't spawned in a while (2.0s)
            if is_strong or (t - last_time > 2.0):
                final_times.append(t)
                last_time = t
    
    # Assign directions Left, Down, Up, Right in a satisfying flow
    directions = ["Left", "Down", "Up", "Right"]
    notes_json = []
    for idx, t in enumerate(final_times):
        d = directions[idx % len(directions)]
        notes_json.append({
            "time": round(float(t), 4),
            "direction": d
        })
        
    chart_data = {
        "bpm": round(bpm, 2),
        "notes": notes_json
    }
    
    with open(output_path, "w") as f:
        json.dump(chart_data, f, indent=2)
        
    print(f"Generated chart with {len(notes_json)} notes to {output_path}")

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python generate_beat_chart.py <audio_path> [output_path]")
        sys.exit(1)
    audio = sys.argv[1]
    out = sys.argv[2] if len(sys.argv) > 2 else None
    generate_chart(audio, out)
