#!/usr/bin/env python3
"""Collect the Ubuntu runtime libraries used directly or through dlopen."""

import json
import re
import shutil
import subprocess
from pathlib import Path


DEST = Path("/bundle")
LIB_DIR = Path("/usr/lib/x86_64-linux-gnu")
# Retain Ubuntu's core C runtime; mixing bundled glibc with desktop services
# and user-installed modules would be less reliable. These ship with Ubuntu.
SYSTEM_LIBS = {
    "libc.so.6", "libm.so.6", "libpthread.so.0", "libdl.so.2", "librt.so.1",
    "libutil.so.1", "libresolv.so.2", "ld-linux-x86-64.so.2",
}
copied: dict[str, Path] = {}


def dependencies(path: Path) -> list[Path]:
    output = subprocess.check_output(["ldd", str(path)], text=True)
    if "not found" in output:
        raise RuntimeError(f"Unresolved dependency of {path}:\n{output}")
    return [Path(match) for match in re.findall(r"(?:=>\s+|^\s*)(/\S+)", output, re.M)]


def copy_library(path: Path, subdir: str = "lib") -> None:
    if path.name in SYSTEM_LIBS:
        return
    destination = DEST / subdir / path.name
    key = str(destination.relative_to(DEST))
    if key in copied:
        if copied[key].resolve() != path.resolve():
            raise RuntimeError(f"Conflicting libraries named {path.name}")
        return
    if not path.is_file():
        raise FileNotFoundError(path)
    copied[key] = path
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(path, destination, follow_symlinks=True)
    for dependency in dependencies(path):
        copy_library(dependency)


def copy_executable(source: Path, name: str) -> None:
    destination = DEST / "bin" / name
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, destination)
    for dependency in dependencies(source):
        copy_library(dependency)


copy_executable(Path("/app/target/release/hearing_test"), "hearing-test")
# The app uses pactl to record volume when wpctl isn't supplied by the desktop.
copy_executable(Path("/usr/bin/pactl"), "pactl")

for name in (
    "libX11.so.6", "libX11-xcb.so.1", "libXcursor.so.1", "libXi.so.6",
    "libXrandr.so.2", "libXext.so.6", "libXfixes.so.3", "libXrender.so.1",
    "libxcb-shape.so.0", "libxcb-xfixes.so.0", "libxcb-render.so.0",
    "libxkbcommon.so.0", "libxkbcommon-x11.so.0", "libwayland-client.so.0",
    "libwayland-cursor.so.0", "libwayland-egl.so.1", "libvulkan.so.1",
    "libvulkan_lvp.so", "libdbus-1.so.3",
):
    copy_library(LIB_DIR / name)

plugins = sorted((LIB_DIR / "alsa-lib").glob("*.so"))
if not plugins:
    raise RuntimeError("ALSA plugins are missing")
for plugin in plugins:
    copy_library(plugin, "lib/alsa-lib")

shutil.copytree("/usr/share/alsa", DEST / "share/alsa", symlinks=False)
shutil.copytree("/usr/share/X11/xkb", DEST / "share/X11/xkb", symlinks=False)
shutil.copytree("/usr/share/X11/locale", DEST / "share/X11/locale", symlinks=False)
# Resolve the ALSA plugin configuration directory inside the bundle. Keep the
# user's asoundrc hooks, but avoid loading incompatible system plugin defaults.
alsa_config = DEST / "share/alsa/alsa.conf"
config_text = alsa_config.read_text()
config_text = config_text.replace('"/usr/etc/alsa/conf.d"', "")
config_text = config_text.replace(
    '"/etc/alsa/conf.d"',
    '{ @func concat strings [ { @func getenv vars [ ALSA_CONFIG_DIR ] '
    'default "/usr/share/alsa" } "/alsa.conf.d" ] }',
)
alsa_config.write_text(config_text)

icd_dir = DEST / "share/vulkan/icd.d"
icd_dir.mkdir(parents=True)
(icd_dir / "lvp.json").write_text(json.dumps({
    "file_format_version": "1.0.0",
    "ICD": {"api_version": "1.1.255", "library_path": "../../../lib/libvulkan_lvp.so"},
}, indent=2) + "\n")

# Include the native libraries' copyright notices, referenced license texts,
# and the build's package versions for attribution and source identification.
licenses = DEST / "licenses"
licenses.mkdir()
for copyright_file in sorted(Path("/usr/share/doc").glob("*/copyright")):
    if copyright_file.is_file():
        shutil.copy2(copyright_file, licenses / (copyright_file.parent.name + ".copyright"))
shutil.copytree("/usr/share/common-licenses", licenses / "common-licenses")
(licenses / "ubuntu-packages.txt").write_text(subprocess.check_output(
    ["dpkg-query", "-W", "-f=${binary:Package}\t${Version}\t${source:Package}\n"], text=True,
))
(DEST / "bundled-libraries.txt").write_text("\n".join(sorted(copied)) + "\n")
shutil.copy2("/app/packaging/AppRun", DEST / "AppRun")
(DEST / "AppRun").chmod(0o755)
(DEST / "README.txt").write_text(
    "Hearing Tracker — Ubuntu 22.04+ x86-64 portable bundle\n\n"
    "Run AppRun, or run the original .run file to extract and launch automatically.\n"
    "Includes ALSA, its audio plugins/configuration, desktop client libraries,\n"
    "and a software Vulkan renderer. No package installation or FUSE is needed.\n"
    "Uses Ubuntu's core C runtime and the running desktop/audio services.\n"
    "Results are saved in the user's data directory, outside this bundle.\n"
    "This is an uncalibrated personal tracker, not a diagnostic medical device.\n"
)
print(f"Bundled {len(copied)} native libraries/plugins.")
