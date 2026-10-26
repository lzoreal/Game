"""BT 客户端目录、检测与交接计划（无 GUI 依赖）。"""

import os
import re

try:
    import winreg
except ImportError:
    winreg = None

THUNDER_PATHS = (
    r"C:\Program Files (x86)\Thunder Network\Thunder\Program\Thunder.exe",
    r"C:\Program Files\Thunder Network\Thunder\Program\Thunder.exe",
    r"D:\Program Files (x86)\Thunder Network\Thunder\Program\Thunder.exe",
    r"D:\Program Files\Thunder Network\Thunder\Program\Thunder.exe",
)

CATALOG = (
    {
        "key": "qbittorrent",
        "name": "qBittorrent",
        "badge": "qB",
        "color": "#3b82f6",
        "desc": "开源、无广告，支持 Windows / macOS / Linux。",
        "url": "https://www.qbittorrent.org/download",
        "recommended": True,
        "exe_name": "qbittorrent.exe",
        "paths": (
            r"{ProgramFiles}\qBittorrent\qbittorrent.exe",
            r"{ProgramFiles(x86)}\qBittorrent\qbittorrent.exe",
            r"{LOCALAPPDATA}\Programs\qBittorrent\qbittorrent.exe",
        ),
        "args": "uri",
        "fallback": None,
    },
    {
        "key": "bitcomet",
        "name": "BitComet",
        "badge": "BC",
        "color": "#0ea5a4",
        "desc": "老牌 BitTorrent 客户端，支持长效种子。",
        "url": "https://www.bitcomet.com/en/download",
        "exe_name": "BitComet.exe",
        "paths": (
            r"{ProgramFiles}\BitComet\BitComet.exe",
            r"{ProgramFiles(x86)}\BitComet\BitComet.exe",
        ),
        "args": "uri",
        "fallback": None,
    },
    {
        "key": "transmission",
        "name": "Transmission",
        "badge": "TR",
        "color": "#22c55e",
        "desc": "轻量、省资源，Windows / macOS / Linux。",
        "url": "https://transmissionbt.com/download/",
        "exe_name": "transmission-qt.exe",
        "paths": (
            r"{ProgramFiles}\Transmission\transmission-qt.exe",
            r"{ProgramFiles(x86)}\Transmission\transmission-qt.exe",
        ),
        "args": "uri",
        "fallback": None,
    },
    {
        "key": "deluge",
        "name": "Deluge",
        "badge": "DL",
        "color": "#f59e0b",
        "desc": "插件化，适合进阶用户。",
        "url": "https://deluge-torrent.org/download/",
        "exe_name": "deluge.exe",
        "paths": (
            r"{ProgramFiles}\Deluge\deluge.exe",
            r"{ProgramFiles(x86)}\Deluge\deluge.exe",
        ),
        "args": "uri",
        "fallback": None,
    },
    {
        "key": "utorrent",
        "name": "uTorrent",
        "badge": "uT",
        "color": "#64748b",
        "desc": "经典轻量客户端；安装时注意取消捆绑组件。",
        "url": "https://www.utorrent.com/downloads/",
        "exe_name": "uTorrent.exe",
        "paths": (
            r"{LOCALAPPDATA}\uTorrent\uTorrent.exe",
            r"{APPDATA}\uTorrent\uTorrent.exe",
            r"{ProgramFiles}\uTorrent\uTorrent.exe",
            r"{ProgramFiles(x86)}\uTorrent\uTorrent.exe",
        ),
        "args": "uri",
        "fallback": None,
    },
    {
        "key": "tixati",
        "name": "Tixati",
        "badge": "TX",
        "color": "#8b5cf6",
        "desc": "免费、功能丰富，支持 Windows / Linux。",
        "url": "https://www.tixati.com/download/",
        "exe_name": "tixati.exe",
        "paths": (
            r"{ProgramFiles}\Tixati\tixati.exe",
            r"{ProgramFiles(x86)}\Tixati\tixati.exe",
        ),
        "args": "uri",
        "fallback": None,
    },
    {
        "key": "biglybt",
        "name": "BiglyBT",
        "badge": "BB",
        "color": "#06b6d4",
        "desc": "开源、功能全面，基于 Azureus。",
        "url": "https://www.biglybt.com/download/",
        "exe_name": "BiglyBT.exe",
        "paths": (
            r"{ProgramFiles}\BiglyBT\BiglyBT.exe",
            r"{ProgramFiles(x86)}\BiglyBT\BiglyBT.exe",
        ),
        "args": "uri",
        "fallback": None,
    },
    {
        "key": "thunder",
        "name": "迅雷",
        "badge": "XL",
        "color": "#eab308",
        "desc": "国内常用；本程序右键菜单已支持联动迅雷。",
        "url": "https://www.xunlei.com/",
        "thunder": True,
        "args": "none",
        "fallback": "shell",
    },
)


class SystemProbe:
    def environment(self):
        return {
            "ProgramFiles": os.environ.get("ProgramFiles", r"C:\Program Files"),
            "ProgramFiles(x86)": os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)"),
            "LOCALAPPDATA": os.environ.get("LOCALAPPDATA", ""),
            "APPDATA": os.environ.get("APPDATA", ""),
        }

    def exists(self, path):
        return bool(path) and os.path.exists(path)

    def app_path(self, exe_name):
        if winreg is None:
            return None
        locations = (
            (winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths"),
            (winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\App Paths"),
            (winreg.HKEY_CURRENT_USER, r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths"),
            (winreg.HKEY_CURRENT_USER, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\App Paths"),
        )
        for root, base in locations:
            try:
                with winreg.OpenKey(root, base + "\\" + exe_name) as key:
                    value = winreg.QueryValue(key, None)
                    if value:
                        value = value.strip('"')
                        if os.path.exists(value):
                            return value
            except OSError:
                continue
        return None

    def magnet_handler(self):
        if winreg is None:
            return None
        try:
            with winreg.OpenKey(winreg.HKEY_CLASSES_ROOT, r"magnet\shell\open\command") as key:
                command = winreg.QueryValue(key, None)
        except OSError:
            return None
        if not command:
            return None
        match = re.match(r'\s*"([^"]+)"', command)
        exe = match.group(1) if match else command.split(" ")[0]
        if exe and os.path.exists(exe):
            return {"exe": exe, "command": command}
        return None

    def thunder_path(self):
        if winreg is not None:
            try:
                with winreg.OpenKey(
                    winreg.HKEY_LOCAL_MACHINE,
                    r"SOFTWARE\Classes\thunder\shell\open\command",
                ) as key:
                    command = winreg.QueryValue(key, None)
                    if command:
                        exe = command.split('"')[1]
                        if os.path.exists(exe):
                            return exe
            except Exception:
                pass
        for path in THUNDER_PATHS:
            if os.path.exists(path):
                return path
        return None


def detect(probe=None):
    probe = probe or SystemProbe()
    env = probe.environment()
    targets = []

    for entry in CATALOG:
        exe = None
        if entry.get("thunder"):
            exe = probe.thunder_path()
        else:
            for template in entry.get("paths", ()):
                path = template.format(**env)
                if probe.exists(path):
                    exe = path
                    break
            if not exe and entry.get("exe_name"):
                exe = probe.app_path(entry["exe_name"])
        if exe:
            targets.append({
                "key": entry["key"],
                "name": entry["name"],
                "kind": "client",
                "exe": exe,
            })

    handler = probe.magnet_handler()
    if handler and handler.get("exe"):
        handler_exe = handler["exe"]
        normalized = os.path.normcase(os.path.abspath(handler_exe))
        if not any(
            os.path.normcase(os.path.abspath(target["exe"])) == normalized
            for target in targets
        ):
            targets.append({
                "key": "__system__",
                "name": os.path.splitext(os.path.basename(handler_exe))[0],
                "kind": "system",
                "exe": handler_exe,
            })

    return targets


def plan(uri, target):
    if not target:
        return None
    if target.get("kind") == "system":
        return {"kind": "shell", "clipboard": uri, "target": target}

    entry = next((item for item in CATALOG if item["key"] == target.get("key")), None)
    if entry is None:
        return None

    args = [uri] if entry.get("args") == "uri" else []
    fallback = (
        {"kind": "shell", "clipboard": uri}
        if entry.get("fallback") == "shell"
        else None
    )
    return {
        "kind": "process",
        "exe": target.get("exe", ""),
        "args": args,
        "clipboard": uri,
        "fallback": fallback,
        "target": target,
    }
