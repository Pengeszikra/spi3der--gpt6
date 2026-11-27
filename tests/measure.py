"""Median wall times from five release processes, plus real PTY traffic and response latency."""
import errno
import fcntl
import os
import pty
import re
import select
import statistics
import struct
import subprocess
import sys
import termios
import time
from pathlib import Path

binary = str(Path(sys.argv[1] if len(sys.argv) > 1 else "target/release/spi3derr").resolve())
frames=240

def wall(args):
    start=time.perf_counter()
    subprocess.run([binary,*args],check=True,stdout=subprocess.DEVNULL)
    return (time.perf_counter()-start)*1000

def timed_scene(scene):
    args=[binary,"--scene",scene,"--cols","120","--rows","40","--bench",str(frames)]
    samples=[]
    for _ in range(5):
        out=subprocess.check_output(args,text=True)
        samples.append(dict((k,float(v)) for k,v in re.findall(r"(\w+)=([\d.]+)",out)))
    row={k:statistics.median(s[k] for s in samples) for k in samples[0]}
    fd,slave=pty.openpty()
    fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack("HHHH",42,120,0,0))
    start=time.perf_counter()
    proc=subprocess.Popen(args+["--bench-stream"],stdin=subprocess.DEVNULL,stdout=slave,stderr=subprocess.PIPE)
    os.close(slave)
    count=0
    while True:
        try:
            chunk=os.read(fd,65536)
            if not chunk:break
            count+=len(chunk)
        except OSError as e:
            if e.errno==errno.EIO:break
            raise
    # wait4 gives peak RSS for this child alone.
    _,status,usage=os.wait4(proc.pid,0)
    proc.returncode=os.waitstatus_to_exitcode(status)
    assert proc.returncode==0,proc.stderr.read()
    elapsed=time.perf_counter()-start
    rss=usage.ru_maxrss/(1024*1024 if sys.platform=="darwin" else 1024)
    os.close(fd)
    assert count==int(row["ansi_bytes"]), (count,row)
    once=statistics.median(wall(["--scene",scene,"--once","--cols","120","--rows","40"]) for _ in range(5))
    print(f'{scene} | {row["load_ms"]:.2f} | {once:.2f} | {rss:.1f} | {row["fps"]:.1f} | {row["ansi_ms"]/frames:.3f} | {frames/elapsed:.1f} | {count/frames:.0f} | {row["order_us"]:.1f} | {row["switch_us"]:.3f}',flush=True)
print("startup_ms",statistics.median(wall(["--version"]) for _ in range(15)))
for name in ("globe","deepsky","lark-4","workshop","betterman","table"):
    timed_scene(name)

# Measure the paced interactive editor separately from the unpaced stream.
# The first sync marker marks a frame reaching the PTY reader.
from workshop import Editor
t = Editor("--scene", "globe")
try:
    t.expect(b"8 objects")
    t.read(.12)
    t.send(b"t")
    t.read(.10)
    delay = []
    for _ in range(20):
        start = time.perf_counter()
        t.send(b"d")
        if not select.select([t.fd], [], [], 3)[0]:
            raise TimeoutError("no response to D")
        assert os.read(t.fd, 65536)
        delay.append(1000 * (time.perf_counter() - start))
        t.send(b" ")
        t.read(.07)
    t.send(b"d")
    t.expect(b"D1")
    start = time.perf_counter()
    frames_out = t.read(2).count(b"\x1b[?2026h")
    print("paced_globe_frames_per_second", round(frames_out / (time.perf_counter()-start), 2))
    print("keydown_to_first_output_ms_p50_p95", round(statistics.median(delay), 2), round(sorted(delay)[18], 2))
    t.send(b" ")
    t.read(.15)
    print("idle_bytes_one_second", len(t.read(1)))
finally:
    t.close()
