#!/usr/bin/env python3
"""A fake ACP agent (protocol v1, newline-delimited JSON-RPC on stdio) for composer's tests.

It writes every message it receives, and its pid, as JSON lines to $FAKE_ACP_LOG. What a prompt
does depends on its text:
- "permission": asks session/request_permission, then answers with the option it got;
- "wait": streams a chunk, then waits for session/cancel and ends the turn `cancelled`;
- "tools": starts the first MCP server session/new gave it and answers with its tool names;
- anything else: a thought, two message chunks and a finished tool call, then `end_turn`.
"""

import json
import os
import queue
import subprocess
import sys
import threading

LOG = os.environ.get("FAKE_ACP_LOG")
write_lock = threading.Lock()
responses = {}
responses_lock = threading.Lock()
cancelled = threading.Event()
next_id = [100]
mcp_servers = []


def log(entry):
    if LOG:
        with open(LOG, "a") as f:
            f.write(json.dumps(entry) + "\n")


def send(message):
    message["jsonrpc"] = "2.0"
    with write_lock:
        sys.stdout.write(json.dumps(message) + "\n")
        sys.stdout.flush()


def update(session, update):
    send({"method": "session/update", "params": {"sessionId": session, "update": update}})


def chunk(kind, text):
    return {"sessionUpdate": kind, "content": {"type": "text", "text": text}}


def request(method, params):
    next_id[0] += 1
    rid = next_id[0]
    waiter = queue.Queue()
    with responses_lock:
        responses[rid] = waiter
    send({"id": rid, "method": method, "params": params})
    return waiter.get()


def prompt(rid, params):
    session = params["sessionId"]
    text = "".join(b.get("text", "") for b in params["prompt"] if b.get("type") == "text")
    if "permission" in text:
        update(session, {"sessionUpdate": "tool_call", "toolCallId": "t1", "title": "Run rm -rf build",
                         "kind": "execute", "status": "pending", "rawInput": {"command": "rm -rf build"}})
        answer = request("session/request_permission", {
            "sessionId": session,
            "toolCall": {"toolCallId": "t1", "title": "Run rm -rf build",
                         "rawInput": {"command": "rm -rf build"}},
            "options": [
                {"optionId": "yes", "name": "Allow", "kind": "allow_once"},
                {"optionId": "always", "name": "Always allow rm", "kind": "allow_always"},
                {"optionId": "no", "name": "Reject", "kind": "reject_once"},
            ],
        })
        outcome = answer.get("result", {}).get("outcome", {})
        chosen = outcome.get("optionId") if outcome.get("outcome") == "selected" else outcome.get("outcome")
        update(session, {"sessionUpdate": "tool_call_update", "toolCallId": "t1",
                         "status": "completed" if chosen in ("yes", "always") else "failed",
                         "content": [{"type": "content", "content": {"type": "text", "text": "removed"}}]})
        update(session, chunk("agent_message_chunk", f"you chose {chosen}"))
        send({"id": rid, "result": {"stopReason": "end_turn"}})
    elif "tools" in text:
        update(session, chunk("agent_message_chunk", ", ".join(mcp_tools(mcp_servers[0]))))
        send({"id": rid, "result": {"stopReason": "end_turn"}})
    elif "wait" in text:
        update(session, chunk("agent_message_chunk", "working"))
        cancelled.wait(30)
        cancelled.clear()
        send({"id": rid, "result": {"stopReason": "cancelled"}})
    else:
        update(session, chunk("agent_thought_chunk", "thinking it over"))
        update(session, chunk("agent_message_chunk", "Hello "))
        update(session, {"sessionUpdate": "tool_call", "toolCallId": "t0", "title": "Read notes.md",
                         "kind": "read", "status": "in_progress", "rawInput": {"path": "notes.md"}})
        update(session, {"sessionUpdate": "tool_call_update", "toolCallId": "t0", "status": "completed",
                         "content": [{"type": "content", "content": {"type": "text", "text": "the notes"}}]})
        update(session, chunk("agent_message_chunk", "world"))
        send({"id": rid, "result": {"stopReason": "end_turn"}})


def mcp_tools(server):
    """The tool names an MCP server (as session/new gave it) lists."""
    env = dict(os.environ)
    env.update({v["name"]: v["value"] for v in server.get("env", [])})
    process = subprocess.Popen([server["command"], *server.get("args", [])], env=env,
                               stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)

    def call(rid, method, params):
        process.stdin.write(json.dumps({"jsonrpc": "2.0", "id": rid, "method": method,
                                        "params": params}) + "\n")
        process.stdin.flush()
        return json.loads(process.stdout.readline())

    call(1, "initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                           "clientInfo": {"name": "fake-acp-agent", "version": "1"}})
    process.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
    tools = call(2, "tools/list", {})["result"]["tools"]
    process.stdin.close()
    process.wait(timeout=10)
    return sorted(tool["name"] for tool in tools)


def main():
    log({"pid": os.getpid()})
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        message = json.loads(line)
        log(message)
        method = message.get("method")
        rid = message.get("id")
        if method is None:
            with responses_lock:
                waiter = responses.pop(rid, None)
            if waiter:
                waiter.put(message)
        elif method == "initialize":
            send({"id": rid, "result": {"protocolVersion": 1, "agentCapabilities": {}, "authMethods": []}})
        elif method == "session/new":
            mcp_servers[:] = message["params"].get("mcpServers", [])
            send({"id": rid, "result": {"sessionId": "fake-session"}})
        elif method == "session/prompt":
            threading.Thread(target=prompt, args=(rid, message["params"]), daemon=True).start()
        elif method == "session/cancel":
            cancelled.set()
        elif rid is not None:
            send({"id": rid, "error": {"code": -32601, "message": f"no method {method}"}})


main()
