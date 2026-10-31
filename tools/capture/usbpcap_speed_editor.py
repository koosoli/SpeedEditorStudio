"""Decode a USBPcap capture of a Speed Editor talking to DaVinci Resolve.

    python usbpcap_speed_editor.py capture.pcap [--json out.jsonl]

Prints every HID report the device sent (keys, wheel, battery) and every
report the host sent to it (LEDs, jog mode, authentication), with the capture
timestamp. Used to measure how Resolve itself drives the Speed Editor, so
our app can reproduce it (docs/FINDINGS.md).

USBPcap stores each USB transfer as a pcap record with link type 249: a
little-endian USBPcap header followed by the transfer data. Only completed
interrupt IN transfers carry device reports; control transfers carry
SET_REPORT/GET_REPORT (feature and output reports).
"""

import argparse
import json
import struct
import sys

LINKTYPE_USBPCAP = 249
TRANSFER_INTERRUPT = 1
TRANSFER_CONTROL = 2
# USBPcap "info" bit 0: the transfer travels from device to host.
INFO_PDO_TO_FDO = 0x01
CONTROL_STAGE_SETUP, CONTROL_STAGE_DATA, CONTROL_STAGE_STATUS, CONTROL_STAGE_COMPLETE = 0, 1, 2, 3


def records(path):
    """Yields (timestamp seconds, packet bytes) for each pcap record."""
    with open(path, "rb") as f:
        header = f.read(24)
        magic = struct.unpack("<I", header[:4])[0]
        if magic == 0xA1B2C3D4:
            endian, divisor = "<", 1e6
        elif magic == 0xA1B23C4D:
            endian, divisor = "<", 1e9
        else:
            raise SystemExit("not a little-endian pcap file (pcapng is not supported: use USBPcapCMD -o file.pcap)")
        linktype = struct.unpack(endian + "I", header[20:24])[0]
        if linktype != LINKTYPE_USBPCAP:
            raise SystemExit(f"link type {linktype} is not USBPcap ({LINKTYPE_USBPCAP})")
        while True:
            rec = f.read(16)
            if len(rec) < 16:
                return
            sec, frac, incl, _orig = struct.unpack(endian + "IIII", rec)
            yield sec + frac / divisor, f.read(incl)


def transfers(path):
    """Yields dicts describing each USB transfer in the capture."""
    for ts, pkt in records(path):
        if len(pkt) < 27:
            continue
        hlen, _irp, status, function, info, bus, device, endpoint, transfer, dlen = struct.unpack(
            "<HQIHBHHBBI", pkt[:27]
        )
        stage = pkt[27] if transfer == TRANSFER_CONTROL and hlen >= 28 else None
        yield {
            "t": ts,
            "status": status,
            "function": function,
            "to_host": bool(info & INFO_PDO_TO_FDO),
            "bus": bus,
            "device": device,
            "endpoint": endpoint,
            "transfer": transfer,
            "stage": stage,
            "data": pkt[hlen:hlen + dlen],
        }


def decode_in(data):
    """Describes a device-to-host report."""
    if not data:
        return None
    rid = data[0]
    if rid == 4 and len(data) >= 13:
        codes = [c for c in struct.unpack("<6H", data[1:13]) if c]
        return {"kind": "keys", "codes": [f"0x{c:02x}" for c in codes]}
    if rid == 3 and len(data) >= 6:
        mode, value = struct.unpack("<Bi", data[1:6])
        return {"kind": "jog", "mode": mode, "value": value, "extra": data[6] if len(data) > 6 else None}
    if rid == 7 and len(data) >= 3:
        return {"kind": "battery", "charging": bool(data[1]), "level": data[2]}
    if rid == 6:
        return {"kind": "auth_reply", "step": data[1] if len(data) > 1 else None}
    return {"kind": "other", "id": rid}


def decode_out(data):
    """Describes a host-to-device report (output or feature)."""
    if not data:
        return None
    rid = data[0]
    if rid == 2 and len(data) >= 5:
        return {"kind": "leds", "bits": f"0x{struct.unpack('<I', data[1:5])[0]:05x}"}
    if rid == 3 and len(data) >= 7:
        mode, value, unknown = struct.unpack("<BiB", data[1:7])
        return {"kind": "set_jog_mode", "mode": mode, "value": value, "param": unknown}
    if rid == 4 and len(data) >= 2:
        return {"kind": "jog_leds", "bits": data[1]}
    if rid == 6:
        return {"kind": "auth", "step": data[1] if len(data) > 1 else None}
    return {"kind": "other", "id": rid}


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("capture")
    parser.add_argument("--json", help="also write one JSON object per report to this file")
    parser.add_argument("--device", type=int, help="only this USB device address")
    args = parser.parse_args()

    out = open(args.json, "w", encoding="utf-8") if args.json else None
    t0 = None
    counts = {}
    for tr in transfers(args.capture):
        if args.device is not None and tr["device"] != args.device:
            continue
        data = tr["data"]
        event = None
        if tr["transfer"] == TRANSFER_INTERRUPT and tr["to_host"] and data:
            event = {"dir": "in", **(decode_in(data) or {})}
        elif tr["transfer"] == TRANSFER_INTERRUPT and not tr["to_host"] and data:
            event = {"dir": "out", **(decode_out(data) or {})}
        elif tr["transfer"] == TRANSFER_CONTROL and data and tr["stage"] in (CONTROL_STAGE_SETUP,) and len(data) >= 8:
            # SET_REPORT with its payload in the same record (USBPcap "complete" form).
            bm, req, wvalue = data[0], data[1], struct.unpack("<H", data[2:4])[0]
            payload = data[8:]
            if req == 0x09 and payload:  # SET_REPORT
                event = {"dir": "out", "via": "control", **(decode_out(payload) or {})}
        elif tr["transfer"] == TRANSFER_CONTROL and data and tr["stage"] == CONTROL_STAGE_DATA:
            if tr["to_host"]:
                event = {"dir": "in", "via": "control", **(decode_in(data) or {})}
            else:
                event = {"dir": "out", "via": "control", **(decode_out(data) or {})}
        if not event:
            continue
        if t0 is None:
            t0 = tr["t"]
        event.update(time=round(tr["t"], 6), rel=round(tr["t"] - t0, 4), device=tr["device"],
                     endpoint=f"0x{tr['endpoint']:02x}", raw=data.hex())
        counts[event.get("kind")] = counts.get(event.get("kind"), 0) + 1
        if out:
            out.write(json.dumps(event) + "\n")
        else:
            print(json.dumps(event))
    print("summary:", json.dumps(counts), file=sys.stderr)


if __name__ == "__main__":
    main()
