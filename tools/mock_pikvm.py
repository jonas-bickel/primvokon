#!/usr/bin/env python3
"""Minimal fake kvmd for development: REST envelope, websocket events and an MJPEG stream.

Usage: python3 tools/mock_pikvm.py [port] [frame1.jpg frame2.jpg ...]
Auth: any X-KVMD-User/X-KVMD-Passwd, Basic auth, or the cookie from POST /api/auth/login.
"""
import base64
import hashlib
import json
import struct
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8085
FRAMES = [open(p, "rb").read() for p in sys.argv[2:]] or [b""]
TOKEN = "796cb83b11de4fcb749bc1bad14a91fb06dede84672b2f847fef1e988e6900de"
WS_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
RECEIVED = []
OCR_CALLS = [0]

STATES = {
    "info_hw_state": {"platform": {"type": "rpi", "base": "Raspberry Pi 4 Model B Rev 1.5", "board": "rpi4", "model": "v3", "serial": "MOCK", "video": "hdmi"},
                      "health": {"cpu": {"percent": 7}, "mem": {"available": 1500000000, "percent": 18.2, "total": 1836331008},
                                 "temp": {"cpu": 45.3}, "throttling": {"raw_flags": 0, "ignore_past": False, "parsed_flags": {"undervoltage": {"now": False, "past": False}}}}},
    "info_system_state": {"kvmd": {"version": "4.120"}, "streamer": {"app": "ustreamer", "version": "6.30", "features": {}}, "kernel": {"system": "Linux", "release": "6.6.31", "version": "#1", "machine": "aarch64"}},
    "info_meta_state": {"server": {"host": "mock-pikvm"}, "kvm": {}},
    "info_extras_state": {"vnc": {"name": "VNC", "description": "", "icon": "", "path": "vnc", "daemon": "kvmd-vnc", "port": 5900, "place": 20, "enabled": True}},
    "wol_state": {"enabled": False, "target": {"ip": "255.255.255.255", "port": 9, "mac": ""}},
    "gpio_model_state": {"scheme": {"inputs": {"led1": {"hw": {"driver": "__gpio__", "pin": 19}}},
                                    "outputs": {"relay1": {"switch": True, "pulse": {"delay": 0.1, "min_delay": 0.1, "max_delay": 5}, "hw": {"driver": "relay", "pin": 0}},
                                                "button1": {"switch": False, "pulse": {"delay": 0.1, "min_delay": 0.1, "max_delay": 0.1}, "hw": {"driver": "__gpio__", "pin": 26}}}},
                         "view": {"header": {"title": "Switches"}, "table": [[{"type": "label", "text": "Relay 1:"}, {"type": "input", "channel": "led1", "color": "green"}, {"type": "output", "channel": "relay1", "text": "Switch"}], None,
                                                                             [{"type": "label", "text": "Button:"}, {"type": "output", "channel": "button1", "text": "Boop"}]]}},
    "gpio_state": {"inputs": {"led1": {"online": True, "state": True}}, "outputs": {"relay1": {"online": True, "state": False, "busy": False}, "button1": {"online": True, "state": False, "busy": False}}},
    "hid_keymaps_state": {"keymaps": {"available": ["de", "en-us"], "default": "en-us"}},
    "hid_state": {"online": True, "busy": False, "enabled": True, "connected": None, "jiggler": {"enabled": True, "active": False, "interval": 60},
                  "keyboard": {"online": True, "leds": {"caps": False, "num": True, "scroll": False}, "outputs": {"active": "usb", "available": ["usb"]}},
                  "mouse": {"online": True, "absolute": True, "outputs": {"active": "usb", "available": ["usb", "usb_rel"]}}},
    "atx_state": {"enabled": True, "busy": False, "leds": {"power": True, "hdd": False}},
    "msd_state": {"enabled": True, "online": True, "busy": False,
                  "storage": {"size": 30000000000, "free": 20000000000, "images": {"ubuntu.iso": {"size": 4000000000, "complete": True, "in_storage": True, "removable": True, "mod": 1.0}}, "parts": {}, "uploading": None, "downloading": None},
                  "drive": {"image": {"name": "ubuntu.iso"}, "connected": True, "cdrom": True, "rw": False}, "features": {"multi": True, "cdrom": True}},
    "streamer_state": {"limits": {"max_fps": 40}, "params": {"desired_fps": 30, "quality": 80, "h264_bitrate": 5000, "h264_gop": 30}, "snapshot": {"saved": None},
                       "features": {"quality": True, "resolution": False, "h264": True},
                       "streamer": {"instance_id": "", "encoder": {"type": "M2M-IMAGE", "quality": 80}, "h264": {"bitrate": 5000, "gop": 30, "fps": 30, "online": True},
                                    "sinks": {"h264": {"has_clients": False}, "jpeg": {"has_clients": True}},
                                    "source": {"online": True, "captured_fps": 30, "desired_fps": 30, "resolution": {"width": 640, "height": 360}},
                                    "stream": {"clients": 1, "clients_stat": {}, "queued_fps": 30}}},
}


def envelope(result):
    return json.dumps({"ok": True, "result": result}).encode()


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):
        sys.stderr.write("[mock] %s %s\n" % (self.command, self.path))

    def authed(self):
        h = self.headers
        return bool(h.get("X-KVMD-User") or h.get("Authorization") or ("auth_token=" + TOKEN) in (h.get("Cookie") or ""))

    def send_json(self, body, status=200, ctype="application/json"):
        self.send_response(status)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        path = urlparse(self.path).path
        length = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(length) if length else b""
        if path == "/api/auth/login":
            self.send_response(200)
            self.send_header("Set-Cookie", "auth_token=%s; Path=/" % TOKEN)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        if not self.authed():
            return self.send_json(b"", 401)
        RECEIVED.append((path, self.path, body[:80]))
        if path.startswith("/api/redfish/"):
            self.send_response(204)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        self.send_json(envelope({}))

    def do_DELETE(self):
        self.send_json(envelope({}))

    def do_PATCH(self):
        self.send_response(204)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def do_GET(self):
        u = urlparse(self.path)
        path, q = u.path, parse_qs(u.query)
        if path == "/api/ws":
            return self.websocket(q)
        if path == "/api/redfish/v1":
            return self.send_json(json.dumps({"@odata.id": "/redfish/v1", "Systems": {"@odata.id": "/redfish/v1/Systems"}}).encode())
        if not self.authed():
            return self.send_json(b"", 401)
        if path == "/api/auth/check":
            return self.send_json(b"")
        if path == "/api/info":
            return self.send_json(envelope({"hw": STATES["info_hw_state"], "system": STATES["info_system_state"], "meta": STATES["info_meta_state"],
                                            "extras": STATES["info_extras_state"], "fan": {}, "auth": {"enabled": True}}))
        if path == "/api/log":
            return self.send_json(b"[2026-09-24 10:00:00 kvmd.service] --- kvmd.apps.kvmd.auth INFO --- Authorized user 'admin'\n", ctype="text/plain")
        if path == "/api/hid":
            return self.send_json(envelope(STATES["hid_state"]))
        if path == "/api/hid/keymaps":
            return self.send_json(envelope(STATES["hid_keymaps_state"]))
        if path == "/api/atx":
            return self.send_json(envelope(STATES["atx_state"]))
        if path == "/api/msd":
            return self.send_json(envelope(STATES["msd_state"]))
        if path == "/api/gpio":
            return self.send_json(envelope({"model": STATES["gpio_model_state"], "state": STATES["gpio_state"]}))
        if path == "/api/streamer":
            return self.send_json(envelope(STATES["streamer_state"]))
        if path == "/api/streamer/ocr":
            return self.send_json(envelope({"ocr": {"enabled": True, "langs": {"available": ["eng"], "default": ["eng"]}}}))
        if path == "/api/streamer/snapshot":
            if q.get("ocr", ["0"])[0] in ("1", "true", "yes"):
                OCR_CALLS[0] += 1
                extra = "Outlook: 1 new mail from Bob (%d)\n" % (OCR_CALLS[0] // 4) if OCR_CALLS[0] % 4 == 0 else ""
                text = "Inbox\n12:%02d\nTeams: Anna: are you joining?\n%s" % (int(time.time()) % 60, extra)
                return self.send_json(text.encode(), ctype="text/plain")
            return self.send_json(FRAMES[int(time.time()) % len(FRAMES)], ctype="image/jpeg")
        if path == "/api/switch":
            return self.send_json(envelope({"model": {"ports": []}}))
        if path == "/api/export/prometheus/metrics":
            return self.send_json(b"pikvm_atx_power 1\n", ctype="text/plain")
        if path.startswith("/api/redfish/v1/Systems"):
            return self.send_json(json.dumps({"Members": [{"@odata.id": "/redfish/v1/Systems/0"}], "PowerState": "On"}).encode())
        if path == "/streamer/stream":
            return self.mjpeg()
        self.send_json(b"", 404)

    def mjpeg(self):
        self.send_response(200)
        self.send_header("Content-Type", "multipart/x-mixed-replace;boundary=boundarydonotcross")
        self.end_headers()
        i = 0
        try:
            while True:
                frame = FRAMES[i % len(FRAMES)]
                i += 1
                self.wfile.write(b"--boundarydonotcross\r\nContent-Type: image/jpeg\r\nContent-Length: %d\r\nX-Timestamp: %f\r\n\r\n" % (len(frame), time.time()))
                self.wfile.write(frame + b"\r\n")
                self.wfile.flush()
                time.sleep(0.1)
        except (BrokenPipeError, ConnectionResetError):
            pass

    # --- websocket -----------------------------------------------------------------
    def ws_send(self, sock, text):
        data = text.encode()
        header = bytearray([0x81])
        n = len(data)
        if n < 126:
            header.append(n)
        elif n < 65536:
            header.append(126)
            header += struct.pack(">H", n)
        else:
            header.append(127)
            header += struct.pack(">Q", n)
        sock.sendall(bytes(header) + data)

    def ws_recv(self, sock):
        head = sock.recv(2)
        if len(head) < 2:
            return None
        opcode = head[0] & 0x0F
        masked = head[1] & 0x80
        n = head[1] & 0x7F
        if n == 126:
            n = struct.unpack(">H", sock.recv(2))[0]
        elif n == 127:
            n = struct.unpack(">Q", sock.recv(8))[0]
        mask = sock.recv(4) if masked else b""
        payload = b""
        while len(payload) < n:
            chunk = sock.recv(n - len(payload))
            if not chunk:
                return None
            payload += chunk
        if masked:
            payload = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
        if opcode == 0x8:
            return None
        return payload.decode(errors="replace") if opcode == 0x1 else ""

    def websocket(self, q):
        if not self.authed():
            return self.send_json(b"", 401)
        key = self.headers.get("Sec-WebSocket-Key", "")
        accept = base64.b64encode(hashlib.sha1((key + WS_GUID).encode()).digest()).decode()
        self.send_response(101)
        self.send_header("Upgrade", "websocket")
        self.send_header("Connection", "Upgrade")
        self.send_header("Sec-WebSocket-Accept", accept)
        self.end_headers()
        sock = self.connection
        for name, ev in STATES.items():
            self.ws_send(sock, json.dumps({"event_type": name, "event": ev}))
        self.ws_send(sock, json.dumps({"event_type": "loop", "event": {}}))
        sock.settimeout(1.0)
        last_state = time.time()
        while True:
            try:
                msg = self.ws_recv(sock)
                if msg is None:
                    break
                if msg:
                    ev = json.loads(msg)
                    if ev.get("event_type") == "ping":
                        self.ws_send(sock, json.dumps({"event_type": "pong", "event": {}}))
                    else:
                        RECEIVED.append(("ws", ev.get("event_type"), msg[:80]))
                        sys.stderr.write("[mock] ws event %s\n" % msg[:120])
            except TimeoutError:
                pass
            except OSError:
                break
            if time.time() - last_state > 3:
                last_state = time.time()
                STATES["atx_state"]["leds"]["hdd"] = not STATES["atx_state"]["leds"]["hdd"]
                try:
                    self.ws_send(sock, json.dumps({"event_type": "atx_state", "event": STATES["atx_state"]}))
                except OSError:
                    break
        self.close_connection = True


if __name__ == "__main__":
    server = ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
    sys.stderr.write("[mock] listening on http://127.0.0.1:%d\n" % PORT)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        pass
