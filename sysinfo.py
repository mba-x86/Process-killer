# Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
"""Process Killer - device / hardware information (no UI)."""
import codecs
import ctypes
import json
import os
import re
import socket
import subprocess
import tempfile
import time
import winreg

import psutil

import core

PS = r"""
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$ErrorActionPreference = 'SilentlyContinue'
$d = @{}
$d.cs = Get-CimInstance Win32_ComputerSystem | Select-Object Manufacturer,Model,Name,PCSystemType,Domain,PartOfDomain,HypervisorPresent
$d.os = Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,BuildNumber,OSArchitecture,@{n='Installed';e={$_.InstallDate.ToString('yyyy-MM-dd')}}
$d.cpu = Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors,MaxClockSpeed,L2CacheSize,L3CacheSize,SocketDesignation,VirtualizationFirmwareEnabled
$d.mem = Get-CimInstance Win32_PhysicalMemory | Select-Object Capacity,Speed,ConfiguredClockSpeed,Manufacturer,PartNumber,SMBIOSMemoryType
$d.gpu = Get-CimInstance Win32_VideoController | Select-Object Name,AdapterRAM,DriverVersion,CurrentHorizontalResolution,CurrentVerticalResolution,CurrentRefreshRate
$d.board = Get-CimInstance Win32_BaseBoard | Select-Object Manufacturer,Product
$d.bios = Get-CimInstance Win32_BIOS | Select-Object Manufacturer,SMBIOSBIOSVersion,@{n='Date';e={$_.ReleaseDate.ToString('yyyy-MM-dd')}}
$d.disk = Get-CimInstance Win32_DiskDrive | Select-Object Model,Size,InterfaceType,MediaType
$d.tpm = Get-CimInstance -Namespace root/cimv2/security/microsofttpm -ClassName Win32_Tpm | Select-Object SpecVersion,IsEnabled_InitialValue,IsActivated_InitialValue,ManufacturerVersion
$d.net = Get-CimInstance Win32_NetworkAdapterConfiguration -Filter "IPEnabled=true" | Select-Object Description,IPAddress,MACAddress
$d | ConvertTo-Json -Depth 4 -Compress
"""

DDR = {20: "DDR", 21: "DDR2", 24: "DDR3", 26: "DDR4", 34: "DDR5"}


def _clean(x):
    """Strip invisible direction marks / BOMs that WMI sometimes prepends."""
    if isinstance(x, str):
        return "".join(ch for ch in x if ch.isprintable()).strip()
    if isinstance(x, list):
        return [_clean(i) for i in x]
    if isinstance(x, dict):
        return {k: _clean(v) for k, v in x.items()}
    return x


def _list(x):
    return [] if x is None else x if isinstance(x, list) else [x]


def fmt_size(b):
    b = float(b or 0)
    for unit in ("B", "KB", "MB", "GB"):
        if b < 1024:
            return f"{b:.0f} {unit}" if unit == "B" else f"{b:.1f} {unit}".replace(".0 ", " ")
        b /= 1024
    return f"{b:.2f} TB".replace(".00 ", " ")


def gpu_vram():
    """GPU name -> dedicated VRAM bytes (Win32_VideoController caps at 4 GB, the registry doesn't)."""
    out = {}
    base = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}"
    try:
        with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, base) as k:
            for i in range(winreg.QueryInfoKey(k)[0]):
                sub = winreg.EnumKey(k, i)
                if not sub.isdigit():
                    continue
                try:
                    with winreg.OpenKey(k, sub) as sk:
                        name = winreg.QueryValueEx(sk, "DriverDesc")[0]
                        mem = winreg.QueryValueEx(sk, "HardwareInformation.qwMemorySize")[0]
                        out[name] = int.from_bytes(mem, "little") if isinstance(mem, bytes) else int(mem)
                except OSError:
                    continue
    except OSError:
        pass
    return out


def _reg_dword(path, name):
    try:
        with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, path) as k:
            return winreg.QueryValueEx(k, name)[0]
    except OSError:
        return None


def firmware_mode():
    try:
        t = ctypes.c_int(0)
        if ctypes.windll.kernel32.GetFirmwareType(ctypes.byref(t)):
            return {1: "Legacy BIOS", 2: "UEFI"}.get(t.value, "Unknown")
    except Exception:  # noqa: BLE001
        pass
    return "Unknown"


def secure_boot():
    v = _reg_dword(r"SYSTEM\CurrentControlSet\Control\SecureBoot\State", "UEFISecureBootEnabled")
    if v is None:
        return "Not supported (Legacy BIOS)" if firmware_mode() == "Legacy BIOS" else "Unavailable"
    return "Enabled" if v == 1 else "Disabled"


DX_CACHE = os.path.join(os.environ.get("APPDATA", "."), "ProcessKiller", "directx.txt")


def directx_cached():
    try:
        return open(DX_CACHE, encoding="utf-8").read().strip()
    except OSError:
        return ""


def directx_text():
    """Slow (~30 s): run dxdiag and return e.g. 'DirectX 12 (feature level 12_2)'. Cached on disk."""
    info = read_dxdiag(start_dxdiag())
    if not info.get("version"):
        return directx_cached() or "Unavailable"
    txt = info["version"] + (f" (feature level {info['feature']})" if info.get("feature") else "")
    if info.get("model"):
        txt += f" · {info['model']}"
    try:
        os.makedirs(os.path.dirname(DX_CACHE), exist_ok=True)
        open(DX_CACHE, "w", encoding="utf-8").write(txt)
    except OSError:
        pass
    return txt


def start_dxdiag():
    """dxdiag is slow (5-20 s); start it first so it runs while we query WMI."""
    path = os.path.join(tempfile.gettempdir(), f"pk_dxdiag_{os.getpid()}.txt")
    try:
        os.remove(path)
    except OSError:
        pass
    try:
        subprocess.Popen(["dxdiag", "/whql:off", "/t", path], creationflags=core.NO_WINDOW)
    except OSError:
        pass
    return path


def read_dxdiag(path, timeout=45):
    end = time.time() + timeout
    text = ""
    while time.time() < end:
        if os.path.exists(path):
            time.sleep(0.6)
            raw = open(path, "rb").read()
            text = raw.decode("utf-16", "replace") if raw[:2] in (codecs.BOM_UTF16_LE, codecs.BOM_UTF16_BE) else raw.decode("utf-8", "replace")
            if "DirectX Version" in text:
                break
        time.sleep(0.5)
    try:
        os.remove(path)
    except OSError:
        pass
    info = {}
    m = re.search(r"DirectX Version:\s*(.+)", text)
    if m:
        info["version"] = m.group(1).strip()
    levels = re.findall(r"Feature Levels:\s*(.+)", text)
    if levels:
        info["feature"] = max((lv.split(",")[0].strip() for lv in levels), key=lambda x: [int(p) if p.isdigit() else 0 for p in x.split("_")])
    models = re.findall(r"Driver Model:\s*(.+)", text)
    if models:
        info["model"] = models[0].strip()
    return info


def _raw():
    try:
        p = subprocess.run(["powershell", "-NoProfile", "-Command", PS], capture_output=True, timeout=90,
                           creationflags=core.NO_WINDOW)
        return _clean(json.loads(p.stdout.decode("utf-8-sig", "replace")))
    except Exception:  # noqa: BLE001
        return {}


def collect():
    """-> list of (section title, [(label, value), ...])"""
    d = _raw()
    vram = gpu_vram()
    cs = (_list(d.get("cs")) or [{}])[0]
    osd = (_list(d.get("os")) or [{}])[0]
    sections = []

    kind = {1: "Desktop", 2: "Laptop", 3: "Workstation", 4: "Enterprise server"}.get(cs.get("PCSystemType"), "")
    dev = [("Device name", cs.get("Name") or socket.gethostname()),
           ("Manufacturer", cs.get("Manufacturer") or "—"), ("Model", cs.get("Model") or "—")]
    if kind:
        dev.append(("Type", kind))
    dev.append(("User", os.environ.get("USERNAME", "—")))
    dev.append(("Domain / workgroup", cs.get("Domain") or "—"))
    sections.append(("Device", dev))

    sections.append(("Operating system", [
        ("Edition", osd.get("Caption") or "Windows"),
        ("Version", f"{osd.get('Version', '—')} (build {osd.get('BuildNumber', '—')})"),
        ("Architecture", osd.get("OSArchitecture") or "—"),
        ("Installed on", osd.get("Installed") or "—")]))

    tpm = (_list(d.get("tpm")) or [None])[0]
    cpus = _list(d.get("cpu"))
    if tpm:
        tpm_txt = f"Version {str(tpm.get('SpecVersion', '?')).split(',')[0]} · " + (
            "enabled" if tpm.get("IsEnabled_InitialValue") else "disabled")
    else:
        tpm_txt = "Not detected" if core.is_admin() else "Unavailable (run as Administrator)"
    virt = cpus[0].get("VirtualizationFirmwareEnabled") if cpus else None
    sections.append(("Security & firmware", [
        ("Firmware mode", firmware_mode()),
        ("Secure Boot", secure_boot()),
        ("TPM", tpm_txt),
        ("Virtualization", "Enabled" if (virt or cs.get("HypervisorPresent")) else "Disabled")]))

    if cpus:
        c = cpus[0]
        rows = [("Model", (c.get("Name") or "—").strip()),
                ("Cores / threads", f"{c.get('NumberOfCores', '—')} cores / {c.get('NumberOfLogicalProcessors', '—')} threads"),
                ("Base speed", f"{(c.get('MaxClockSpeed') or 0) / 1000:.2f} GHz")]
        caches = []
        if c.get("L2CacheSize"):
            caches.append(f"L2 {c['L2CacheSize'] / 1024:.0f} MB")
        if c.get("L3CacheSize"):
            caches.append(f"L3 {c['L3CacheSize'] / 1024:.0f} MB")
        if caches:
            rows.append(("Cache", " · ".join(caches)))
        if c.get("SocketDesignation"):
            rows.append(("Socket", c["SocketDesignation"]))
        if len(cpus) > 1:
            rows.insert(0, ("Sockets", str(len(cpus))))
        sections.append(("Processor", rows))

    mems = _list(d.get("mem"))
    rows = [("Total", fmt_size(psutil.virtual_memory().total))]
    if mems:
        speed = max((m.get("ConfiguredClockSpeed") or m.get("Speed") or 0) for m in mems)
        kinds = {DDR.get(m.get("SMBIOSMemoryType"), "") for m in mems} - {""}
        sizes = sorted({m.get("Capacity") or 0 for m in mems})
        mod = f"{len(mems)} × {fmt_size(sizes[-1])}" if len(sizes) == 1 else f"{len(mems)} modules"
        rows.append(("Modules", f"{mod} {' / '.join(kinds)}{f'-{speed}' if speed else ''}".strip()))
        brands = sorted({(m.get('Manufacturer') or '').strip() for m in mems} - {"", "Unknown"})
        if brands:
            rows.append(("Manufacturer", ", ".join(brands)))
        parts = sorted({(m.get('PartNumber') or '').strip() for m in mems} - {""})
        if parts:
            rows.append(("Part number", ", ".join(parts)))
    sw = psutil.swap_memory()
    rows.append(("Page file", fmt_size(sw.total)))
    sections.append(("Memory", rows))

    gpus = _list(d.get("gpu"))
    if gpus:
        rows = []
        for g in gpus:
            name = g.get("Name") or "Graphics adapter"
            mem = vram.get(name) or g.get("AdapterRAM") or 0
            rows.append((name, fmt_size(mem) + " VRAM" if mem else "shared memory"))
            if g.get("DriverVersion"):
                rows.append(("   Driver", g["DriverVersion"]))
        for g in gpus:
            if g.get("CurrentHorizontalResolution"):
                rows.append(("Display", f"{g['CurrentHorizontalResolution']} × {g['CurrentVerticalResolution']} "
                                        f"@ {g.get('CurrentRefreshRate') or '—'} Hz"))
                break
        rows.append(("DirectX", directx_cached() or "Detecting…"))
        sections.append(("Graphics", rows))

    board = (_list(d.get("board")) or [{}])[0]
    bios = (_list(d.get("bios")) or [{}])[0]
    if board or bios:
        sections.append(("Motherboard & BIOS", [
            ("Motherboard", f"{board.get('Manufacturer', '')} {board.get('Product', '')}".strip() or "—"),
            ("BIOS", f"{bios.get('Manufacturer', '')} {bios.get('SMBIOSBIOSVersion', '')}".strip() or "—"),
            ("BIOS date", bios.get("Date") or "—")]))

    disks = _list(d.get("disk"))
    if disks:
        sections.append(("Storage devices", [
            ((x.get("Model") or "Disk").strip(), f"{fmt_size(x.get('Size'))} · {x.get('InterfaceType') or ''}".strip(" ·"))
            for x in disks]))

    nets = _list(d.get("net"))
    if nets:
        rows = []
        for n in nets[:3]:
            ips = _list(n.get("IPAddress"))
            rows.append((n.get("Description") or "Adapter", ips[0] if ips else "—"))
            if n.get("MACAddress"):
                rows.append(("   MAC address", n["MACAddress"]))
        sections.append(("Network", rows))

    bat = psutil.sensors_battery()
    if bat:
        state = "charging" if bat.power_plugged else "on battery"
        sections.append(("Battery", [("Charge", f"{bat.percent:.0f}% ({state})")]))
    return sections


def uptime_text():
    s = int(time.time() - psutil.boot_time())
    d, s = divmod(s, 86400)
    h, s = divmod(s, 3600)
    m = s // 60
    return (f"{d}d " if d else "") + f"{h}h {m}m"


def uptime_days():
    return (time.time() - psutil.boot_time()) / 86400


def report_text(sections):
    lines = ["Process Killer — system report", ""]
    for title, rows in sections:
        lines.append(f"[{title}]")
        lines += [f"  {k.strip():<22} {v}" for k, v in rows]
        lines.append("")
    return "\n".join(lines)
