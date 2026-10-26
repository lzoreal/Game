# -*- mode: python ; coding: utf-8 -*-
"""PyInstaller build for GameMagNet.

Produces both release artifacts from a single run:

    dist/GameMagNet.exe          one-file executable
    dist/GameMagNet/             portable one-folder build

PySide6's QtQml hook pulls in the whole Qt QML tree (every Controls style,
VirtualKeyboard, Pdf, Labs, ...) and every QML plugin's Qt DLLs. The filters
below keep only what ``qml/`` actually imports and then drop Qt DLLs that are
not reachable from the remaining Python extensions and QML plugins.
"""

import os
import struct
from pathlib import Path

ROOT = Path(SPECPATH)
ICON = str(ROOT / "assets" / "gamemagnet.ico")

# Python bindings that this app never imports. Their Qt libraries are still
# collected when Qt itself needs them (for example Qt6Network is a dependency
# of Qt6Quick), so excluding the bindings only drops the Python extension.
QT_CONFIG_EXCLUDES = [
    "PySide6.Qt3DAnimation",
    "PySide6.Qt3DCore",
    "PySide6.Qt3DExtras",
    "PySide6.Qt3DInput",
    "PySide6.Qt3DLogic",
    "PySide6.Qt3DRender",
    "PySide6.QtBluetooth",
    "PySide6.QtCharts",
    "PySide6.QtDataVisualization",
    "PySide6.QtDesigner",
    "PySide6.QtGraphs",
    "PySide6.QtGraphsWidgets",
    "PySide6.QtHelp",
    "PySide6.QtHttpServer",
    "PySide6.QtLocation",
    "PySide6.QtMultimedia",
    "PySide6.QtMultimediaWidgets",
    "PySide6.QtNfc",
    "PySide6.QtPdf",
    "PySide6.QtPdfWidgets",
    "PySide6.QtPositioning",
    "PySide6.QtPrintSupport",
    "PySide6.QtQuick3D",
    "PySide6.QtQuickTest",
    "PySide6.QtQuickWidgets",
    "PySide6.QtRemoteObjects",
    "PySide6.QtScxml",
    "PySide6.QtSensors",
    "PySide6.QtSerialBus",
    "PySide6.QtSerialPort",
    "PySide6.QtSpatialAudio",
    "PySide6.QtSql",
    "PySide6.QtStateMachine",
    "PySide6.QtSvg",
    "PySide6.QtSvgWidgets",
    "PySide6.QtTest",
    "PySide6.QtTextToSpeech",
    "PySide6.QtUiTools",
    "PySide6.QtVirtualKeyboard",
    "PySide6.QtWebChannel",
    "PySide6.QtWebEngineCore",
    "PySide6.QtWebEngineQuick",
    "PySide6.QtWebEngineWidgets",
    "PySide6.QtWebSockets",
    "PySide6.QtWidgets",
    "PySide6.QtXml",
]

a = Analysis(
    [str(ROOT / "main.py")],
    pathex=[str(ROOT)],
    binaries=[],
    datas=[
        (str(ROOT / "qml"), "qml"),
        (str(ROOT / "assets"), "assets"),
    ],
    hiddenimports=[],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=QT_CONFIG_EXCLUDES,
    noarchive=False,
)

# ---------------------------------------------------------------------------
# QML pruning
# ---------------------------------------------------------------------------
# Everything directly inside these QML module directories is kept.
QML_KEEP_DIRS = {
    "PySide6/qml/QtCore",
    "PySide6/qml/QtQml",
    "PySide6/qml/QtQml/Models",
    "PySide6/qml/QtQuick",
    "PySide6/qml/QtQuick/Controls",
    "PySide6/qml/QtQuick/Templates",
    "PySide6/qml/QtQuick/Layouts",
    "PySide6/qml/QtQuick/Dialogs",
    "PySide6/qml/QtQuick/Effects",
    "PySide6/qml/QtQuick/Window",
    "PySide6/qml/QtQuick/Shapes",
}

# These modules (and their subdirectories) are kept recursively.
QML_KEEP_TREES = {
    "PySide6/qml/QtQuick/Controls/Basic",
    "PySide6/qml/QtQuick/Controls/impl",
    "PySide6/qml/QtQuick/Dialogs",
    "PySide6/qml/Qt/labs/folderlistmodel",
}

# Qt plugins that are loaded at runtime by name, so no import graph points at
# them. ``platforms/qwindows`` is the real Windows plugin, the rest are the
# only plugin directories the app uses.
PLUGIN_KEEP = {
    "platforms/qwindows.dll",
    "platforms/qoffscreen.dll",
    "imageformats/qico.dll",
    "tls/qschannelbackend.dll",
    "networkinformation/qnetworklistmanager.dll",
}

# Duplicated OpenSSL libraries that PyInstaller picks up from Git for Windows.
# Python's own copies (libcrypto-3.dll / libssl-3.dll) remain.
DROP_BINARIES = {
    "libcrypto-3-x64.dll",
    "libssl-3-x64.dll",
    "PySide6/opengl32sw.dll",
}


def _norm(path):
    return path.replace("\\", "/")


def _keep_qml(dest):
    if dest.endswith(".qmltypes"):
        return False
    for tree in QML_KEEP_TREES:
        if dest.startswith(tree + "/"):
            return True
    return dest.rsplit("/", 1)[0] in QML_KEEP_DIRS


def _keep_entry(entry):
    dest = _norm(entry[0])
    if "/translations/" in "/" + dest or dest.endswith(".qm"):
        return False
    if dest in DROP_BINARIES:
        return False
    if dest.startswith("PySide6/plugins/"):
        return dest[len("PySide6/plugins/"):] in PLUGIN_KEEP
    if dest.startswith("PySide6/qml/"):
        return _keep_qml(dest)
    return True


a.datas = [e for e in a.datas if _keep_entry(e)]
a.binaries = [e for e in a.binaries if _keep_entry(e)]


# ---------------------------------------------------------------------------
# Prune Qt DLLs that nothing loads anymore
# ---------------------------------------------------------------------------
def _pe_imports(path):
    """Return the DLL names imported by a PE file (empty for non-PE files)."""
    try:
        with open(path, "rb") as handle:
            data = handle.read()
    except OSError:
        return []
    if data[:2] != b"MZ" or len(data) < 0x40:
        return []
    pe_offset = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe_offset:pe_offset + 4] != b"PE\0\0":
        return []
    section_count = struct.unpack_from("<H", data, pe_offset + 6)[0]
    optional = pe_offset + 24
    optional_size = struct.unpack_from("<H", data, pe_offset + 20)[0]
    if struct.unpack_from("<H", data, optional)[0] == 0x20B:
        import_dir = optional + 112
    else:
        import_dir = optional + 96
    import_rva, _ = struct.unpack_from("<II", data, import_dir + 8)
    if not import_rva:
        return []
    sections = []
    for index in range(section_count):
        header = optional + optional_size + index * 40
        virtual_address, virtual_size = struct.unpack_from("<II", data, header + 12)
        raw_pointer = struct.unpack_from("<I", data, header + 20)[0]
        sections.append((virtual_address, max(virtual_size, 0x1000), raw_pointer))

    def to_offset(rva):
        for virtual_address, virtual_size, raw_pointer in sections:
            if virtual_address <= rva < virtual_address + virtual_size:
                return rva - virtual_address + raw_pointer
        return None

    offset = to_offset(import_rva)
    if offset is None:
        return []
    names = []
    while True:
        name_rva = struct.unpack_from("<I", data, offset + 12)[0]
        if not name_rva:
            break
        name_offset = to_offset(name_rva)
        if name_offset is None:
            break
        end = data.find(b"\0", name_offset)
        if end < 0:
            break
        names.append(data[name_offset:end].decode("ascii", "replace"))
        offset += 20
    return names


def _is_qt_binary(dest):
    lowered = dest.lower()
    return lowered.startswith("pyside6/") and lowered.endswith((".dll", ".pyd", ".exe"))


def _prune_qt_binaries(entries):
    qt_binaries = {}
    for entry in entries:
        dest = _norm(entry[0])
        if _is_qt_binary(dest):
            qt_binaries.setdefault(dest.rsplit("/", 1)[-1].lower(), []).append(entry)

    roots = set()
    for entry in entries:
        dest = _norm(entry[0])
        if not _is_qt_binary(dest):
            continue
        name = dest.rsplit("/", 1)[-1].lower()
        if name.endswith(".pyd"):
            roots.add(name)
        elif "/qml/" in dest.lower():
            roots.add(name)
        elif name in {
            "pyside6.abi3.dll",
            "pyside6qml.abi3.dll",
            "shiboken6.abi3.dll",
        }:
            roots.add(name)
    for plugin in PLUGIN_KEEP:
        roots.add(plugin.rsplit("/", 1)[-1].lower())

    reachable = set()
    pending = list(roots)
    while pending:
        name = pending.pop()
        if name in reachable:
            continue
        reachable.add(name)
        for entry in qt_binaries.get(name, ()):
            for imported in _pe_imports(entry[1]):
                imported = imported.lower()
                if imported in qt_binaries and imported not in reachable:
                    pending.append(imported)

    kept = []
    dropped_sources = set()
    for entry in entries:
        dest = _norm(entry[0])
        if _is_qt_binary(dest) and dest.rsplit("/", 1)[-1].lower() not in reachable:
            dropped_sources.add(entry[1])
            continue
        kept.append(entry)

    dropped_size = 0
    for source in dropped_sources:
        try:
            dropped_size += os.path.getsize(source)
        except OSError:
            pass
    print(
        "[GameMagNet] pruned %d unreachable Qt binaries (%.1f MB)"
        % (len(entries) - len(kept), dropped_size / 1e6)
    )
    return kept


a.binaries = _prune_qt_binaries(a.binaries)

# ---------------------------------------------------------------------------
# Targets
# ---------------------------------------------------------------------------
pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.datas,
    [],
    name="GameMagNet",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=False,
    upx_exclude=[],
    runtime_tmpdir=None,
    console=False,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    icon=ICON,
)

onedir_exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name="GameMagNet",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=False,
    upx_exclude=[],
    console=False,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    icon=ICON,
)

coll = COLLECT(
    onedir_exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=False,
    upx_exclude=[],
    name="GameMagNet",
)
