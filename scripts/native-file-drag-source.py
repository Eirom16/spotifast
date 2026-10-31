#!/usr/bin/env python3
"""Owned GTK/Xdnd source for native file-ingress QA, inside a private display.

Receives only the generated CC0 fixture. This is an actual GTK drag source;
the caller drives its pointer with XTest and verifies Doremi's real output.
GTK reference: https://docs.gtk.org/gtk3/method.Widget.drag_source_set.html
"""
import argparse
import json
import os
from pathlib import Path

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gdk, GLib, Gtk

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("fixture", type=Path)
args = parser.parse_args()
fixture = args.fixture.resolve(strict=True)
assert fixture.is_relative_to(ROOT / "target") and fixture.is_file()
assert fixture.name == "Original Café estéreo.wav", "Only the owned QA fixture"
assert os.environ.get("DOREMI_DATA_DIR"), "Private QA environment required"
window = Gtk.Window(title="Doremi QA — own CC0 drag source")
window.set_default_size(280, 120)
window.move(700, 150)
source = Gtk.EventBox()
source.add(Gtk.Label(label="Original CC0 WAV\nDrag to Doremi"))
source.drag_source_set(Gdk.ModifierType.BUTTON1_MASK,
                       [Gtk.TargetEntry.new("text/uri-list", 0, 0)],
                       Gdk.DragAction.COPY)


def provide(_widget, _context, selection, _info, _time):
    assert selection.set_uris([fixture.as_uri()])
    print(json.dumps({"event": "provided-owned-uri"}), flush=True)


def ended(*_args):
    print(json.dumps({"event": "drag-ended"}), flush=True)
    GLib.timeout_add(250, Gtk.main_quit)


source.connect("drag-data-get", provide)
source.connect("drag-end", ended)
window.connect("destroy", Gtk.main_quit)
window.add(source)
window.show_all()
print(json.dumps({"event": "ready", "pid": os.getpid()}), flush=True)
GLib.timeout_add_seconds(20, Gtk.main_quit)
Gtk.main()
