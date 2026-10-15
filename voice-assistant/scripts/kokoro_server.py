"""Keep a Kokoro TTS model loaded and synthesize speech on request.

Started by the voice assistant (tts.engine = "kokoro"); embedded in the binary and
written to the data directory at startup.

Usage: python kokoro_server.py MODEL.onnx VOICES.bin

Protocol (one request per line on stdin):
  request:  {"text": "...", "voice": "af_sky", "speed": 1.0, "lang": "en-us"}
  response: "OK <byte_count> <sample_rate>\n" followed by that many bytes of
            16-bit little-endian mono PCM, or "ERR <message>\n"
The line "READY <voice,voice,...>\n" is printed once the model is loaded.
"""
import json
import sys

import numpy as np
from kokoro_onnx import Kokoro


def main():
    model_path, voices_path = sys.argv[1], sys.argv[2]
    kokoro = Kokoro(model_path, voices_path)
    out = sys.stdout.buffer
    out.write(f"READY {','.join(sorted(kokoro.get_voices()))}\n".encode())
    out.flush()

    for line in sys.stdin:
        if not line.strip():
            continue
        try:
            request = json.loads(line)
            samples, rate = kokoro.create(
                request["text"],
                voice=request["voice"],
                speed=float(request.get("speed", 1.0)),
                lang=request.get("lang", "en-us"),
            )
            pcm = (np.clip(samples, -1.0, 1.0) * 32767).astype("<i2").tobytes()
            out.write(f"OK {len(pcm)} {rate}\n".encode())
            out.write(pcm)
        except Exception as e:  # report and keep serving
            message = " ".join(str(e).split()) or type(e).__name__
            out.write(f"ERR {message}\n".encode())
        out.flush()


if __name__ == "__main__":
    main()
