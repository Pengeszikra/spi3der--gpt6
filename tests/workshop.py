"""Exercise the released executable in a PTY and through independent MCP pipes.
Run python3 tests/workshop.py [binary] [--write-table FILE]. No third-party modules.
"""
import argparse
import errno
import fcntl
import json
import math
import os
import pty
import select
import struct
import subprocess
import tempfile
import termios
import time
import zlib
from xml.etree import ElementTree
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("binary", nargs="?", default="target/release/spi3derr")
parser.add_argument("--write-table", type=Path)
opt = parser.parse_args()
binary = str(Path(opt.binary).resolve())

class Editor:
    def __init__(self, *args):
        self.fd, slave = pty.openpty()
        self.set_size(120, 42)
        self.proc = subprocess.Popen([binary, *args], stdin=slave, stdout=slave, stderr=slave)
        os.close(slave)
        self.buffer = b""
    def set_size(self, w, h):
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", h, w, 0, 0))
    def send(self, keys):
        os.write(self.fd, keys)
    def read(self, duration):
        data = bytearray()
        end = time.monotonic() + duration
        while (left := end-time.monotonic()) > 0 and select.select([self.fd], [], [], left)[0]:
            try:
                data.extend(os.read(self.fd, 65536))
            except OSError as e:
                if e.errno == errno.EIO: break
                raise
        self.buffer += data
        return bytes(data)
    def expect(self, word, seconds=3):
        end = time.monotonic() + seconds
        while word not in self.buffer and time.monotonic() < end:
            self.read(0.025)
        assert word in self.buffer, (word, self.buffer[-450:])
        self.buffer = self.buffer.split(word, 1)[1]
    def close(self):
        self.send(b"\x03")
        try:
            deadline=time.monotonic()+3
            while self.proc.poll() is None and time.monotonic()<deadline: self.read(.05)
            assert self.proc.poll()==0
        finally:
            if self.proc.poll() is None: self.proc.kill()
            os.close(self.fd)

class Client:
    def __init__(self, session):
        self.p = subprocess.Popen([binary, "--mcp", "--session", str(session)], stdin=subprocess.PIPE,
                                  stdout=subprocess.PIPE, text=True)
        self.counter = 0
        init = self.rpc("initialize", {"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"acceptance","version":"1"}})
        assert init["capabilities"]["tools"] == {}
        assert "add_mesh" in [t["name"] for t in self.rpc("tools/list")["tools"]]
    def rpc(self, method, params=None):
        self.counter += 1
        self.p.stdin.write(json.dumps(dict(jsonrpc="2.0", id=self.counter, method=method, params=params or {}))+"\n")
        self.p.stdin.flush()
        reply=json.loads(self.p.stdout.readline())
        assert reply["id"]==self.counter and "error" not in reply,reply
        return reply["result"]
    def tool(self, name, args=None, error=False):
        reply=self.rpc("tools/call", {"name":name, "arguments":args or {}})
        assert reply["isError"]==error,reply
        return reply["content"][0]["text"]
    def close(self):
        self.p.stdin.close()
        assert self.p.wait(timeout=3)==0

def box(name, center, size):
    return dict(shape="box",name=name,center=center,size=size,material="wood")

def table_and_cup(path):
    editor=Editor("--session",str(path))
    editor.expect(b"0 objects")
    c=Client(path)
    c2=Client(path)
    try:
        old=json.loads(c2.tool("scene_state"))["revision"]
        parts=[box("Tabletop",[0,0.75,0],[1.6,0.08,0.85])]
        parts.extend(box(f"Leg {x} {z}",[x,0.355,z],[0.075,0.71,0.075])
                     for x in [-0.69,0.69] for z in [-0.33,0.33])
        c.tool("add_primitives",dict(objects=parts,revision=old))
        editor.expect(b"5 objects")
        editor.send(b"\rTabletop\r")
        editor.expect(b"5 objects | Tabletop")
        # The stale writer cannot accidentally overwrite the five-piece table.
        assert "conflict" in c2.tool("scene_command",{"command":"/new","revision":old},True)
        c.tool("lathe_profile",dict(name="Coffee mug",origin=[0.24,0.79,0],material="plaster",
              segments=32,profile=[[0,0.038],[0.006,0.044],[0.103,0.05],[0.105,0.046],[0.010,0.04],[0.010,0]]))
        editor.expect(b"6 objects | Tabletop")
        # A swept C-shaped tube meets the wall twice and leaves the center open.
        points=[]; faces=[]
        for j in range(17):
            t=math.pi*j/16
            for k in range(8):
                a=2*math.pi*k/8
                points.append([0.283+(.044+.006*math.cos(a))*math.sin(t),
                               .846+(.033+.006*math.cos(a))*math.cos(t),.006*math.sin(a)])
        for j in range(16):
            for k in range(8):
                a=j*8+k;b=j*8+(k+1)%8;faces.append([a,b,b+8,a+8])
        faces += [list(reversed(range(8))),list(range(128,136))]
        c.tool("add_mesh",dict(name="Mug handle",material="plaster",positions=points,faces=faces))
        editor.expect(b"7 objects | Tabletop")
        latest=path.read_bytes()
        path.write_bytes(b'{"format":')
        editor.expect(b"JSON")
        # Last valid mesh can still be targeted while a non-cooperating writer has an incomplete file.
        editor.send(b"\rCoffee mug\r")
        editor.expect(b"7 objects | Coffee mug")
        path.write_bytes(latest+b"\n")
        editor.expect(b"scene reloaded")
        editor.send(b"q3a")
        editor.expect(b"Q edit")
        editor.send(b"gx0.02\r")
        time.sleep(.25)
        state=json.loads(c2.tool("scene_state"))
        edited=next(o for o in state["objects"] if o["name"]=="Coffee mug")
        assert abs(edited["center"][0]-.26)<1e-6, edited
        editor.send(b"u")
        time.sleep(.25)
        assert abs(next(o for o in json.loads(c.tool("scene_state"))["objects"] if o["name"]=="Coffee mug")["center"][0]-.24)<1e-6
        if opt.write_table: opt.write_table.write_bytes(path.read_bytes())
    finally:
        editor.close();c.close();c2.close()

def orbit_and_panels():
    t=Editor("--scene","globe")
    try:
        t.expect(b"8 objects");t.read(.1)
        assert t.read(.28)==b"", "idle screen wrote bytes"
        t.send(b"td");t.expect(b"D1")
        data=t.read(.42)
        assert data.count(b"\x1b[?2026h")>=5
        t.send(b" ");t.expect(b"D0");t.read(.1)
        assert not t.read(.25), "Space did not stop animation"
        t.send(b"\r/load\rdeep")
        t.expect(b"deepsky")
        t.send(b"\x1b[B\x1b[A\r")
        t.expect(b"5149 objects")
        t.send(b"\rSun\r");t.expect(b"5149 objects | Sun")
        t.send(b"\t\x1b[Z");t.read(.12)
        t.send(b"\r/help bevel\r");t.expect(b"Help")
        t.send(b"\x1b[6~\t\x1b");t.read(.12)
        t.send(b"p");t.expect(b"Materials")
        t.send(b"\x1b[B\t\x1b[Cp");t.read(.12)
        t.set_size(84,27);t.expect(b"\x1b[2J")
    finally:
        t.close()
def exported_images(tmp):
    script = tmp / "export.txt"
    script.write_text("/png 128\n/svg 128\n/gif 4 1\n/anim 4 1\n")
    env = dict(os.environ, SPI3DER_STORE=str(tmp / "store"))
    subprocess.run([binary, "--scene", "globe", "--script", str(script), "--once",
                    "--cols", "48", "--rows", "16"], env=env, check=True, stdout=subprocess.DEVNULL)
    files = list((tmp / "store" / "exports").iterdir())
    assert len(files) == 4
    png = next(p for p in files if p.suffix == ".png").read_bytes()
    assert png.startswith(bytes.fromhex("89504e470d0a1a0a"))
    w, h = struct.unpack(">II", png[16:24])
    i = 8
    compressed = bytearray()
    while i < len(png):
        n, tag = struct.unpack(">I4s", png[i:i+8])
        data = png[i+8:i+8+n]
        assert zlib.crc32(tag+data) == int.from_bytes(png[i+8+n:i+12+n], "big")
        if tag == b"IDAT": compressed.extend(data)
        i += 12+n
    raw = zlib.decompress(compressed)
    assert w == 128 and len(raw) == (w*4+1)*h
    alphas = {raw[y*(w*4+1)+4+x*4] for x in range(w) for y in range(h)}
    assert alphas == {0, 255}
    svgs = [ElementTree.parse(p) for p in files if p.suffix == ".svg"]
    assert sorted(len(root.findall("{*}g")) for root in svgs) == [0, 4]
    assert all(root.findall(".//{*}polygon") for root in svgs)
    gif = next(p for p in files if p.suffix == ".gif").read_bytes()
    assert gif[:6] == b"GIF89a" and gif[-1] == 0x3b
    assert gif.count(bytes.fromhex("21f9040019000000")) == 4

if __name__=="__main__":
    with tempfile.TemporaryDirectory() as tmp:
        table_and_cup(Path(tmp)/"live.3draft")
        orbit_and_panels()
        exported_images(Path(tmp))
    print("PTY orbit, idle, resize, catalog, help, palette, two MCP clients, invalid reload, Q edit and undo: passed")
