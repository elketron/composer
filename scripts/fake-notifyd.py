"""A stand-in org.freedesktop.Notifications daemon for the app's notify_test.dart: logs every
notification as JSON lines and, like a user, clicks "approve" on notifications that have it.

    dbus-run-session -- bash -c 'python3 scripts/fake-notifyd.py /tmp/notes.jsonl & sleep 1; <run the test>'

Unset WAYLAND_DISPLAY / DISPLAY for the private bus, or the portals it starts attach to your desktop."""
import json, sys
from gi.repository import Gio, GLib

LOG = sys.argv[1]
XML = """<node><interface name="org.freedesktop.Notifications">
<method name="GetCapabilities"><arg direction="out" type="as"/></method>
<method name="Notify"><arg type="s"/><arg type="u"/><arg type="s"/><arg type="s"/><arg type="s"/>
<arg type="as"/><arg type="a{sv}"/><arg type="i"/><arg direction="out" type="u"/></method>
<method name="CloseNotification"><arg type="u"/></method>
<method name="GetServerInformation"><arg direction="out" type="s"/><arg direction="out" type="s"/>
<arg direction="out" type="s"/><arg direction="out" type="s"/></method>
<signal name="ActionInvoked"><arg type="u"/><arg type="s"/></signal>
<signal name="NotificationClosed"><arg type="u"/><arg type="u"/></signal>
</interface></node>"""
next_id = [1]

def log(entry):
    with open(LOG, "a") as f:
        f.write(json.dumps(entry) + "\n")

def call(conn, sender, path, iface, method, params, inv):
    if method == "GetCapabilities":
        inv.return_value(GLib.Variant("(as)", (["actions", "body"],)))
    elif method == "GetServerInformation":
        inv.return_value(GLib.Variant("(ssss)", ("fake", "test", "1", "1.2")))
    elif method == "CloseNotification":
        log({"closed": params.unpack()[0]})
        inv.return_value(None)
    elif method == "Notify":
        app, replaces, icon, summary, body, actions, hints, timeout = params.unpack()
        nid = replaces or next_id[0]
        next_id[0] += 1
        log({"id": nid, "summary": summary, "body": body, "actions": actions})
        inv.return_value(GLib.Variant("(u)", (nid,)))
        keys = actions[0::2]
        if "approve" in keys:
            def click():
                log({"clicked": "approve", "id": nid})
                conn.emit_signal(sender, "/org/freedesktop/Notifications", "org.freedesktop.Notifications",
                                 "ActionInvoked", GLib.Variant("(us)", (nid, "approve")))
                return False
            GLib.timeout_add(2000, click)

def acquired(conn, name):
    conn.register_object("/org/freedesktop/Notifications", Gio.DBusNodeInfo.new_for_xml(XML).interfaces[0], call)

Gio.bus_own_name(Gio.BusType.SESSION, "org.freedesktop.Notifications", 0, acquired, None, None)
GLib.MainLoop().run()
