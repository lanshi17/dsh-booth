#!/usr/bin/env python3
"""A fixture server for the console: the management API routes, the same-origin guard, and
one ticket, one thread, one accepted decision and one ignored greeting. `python3 dev/mock-api.py 3899` then
`booth --url http://127.0.0.1:3899`."""
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse

ROW = {
 "id":"t-7f3a9c2e1b","title":"Fix the flaky retry test","state":"open","stage":"person",
 "action":None,"note":"waiting for your answer on the design","strength":"hard",
 "actions":{"retry":False,"stop":True},"assignments":1,"inputs":2,"undelivered":0,
 "sessionId":"dsh-superagent-t-7f3a","directory":"/home/me/.dsh/state/superagent/work/fix-flaky",
 "summary":"Reproduced the race; two options for the fix.",
 "createdAt":"2026-11-04T09:00:00.000Z","updatedAt":"2026-11-04T10:12:00.000Z",
 "threads":[{"key":"github:o/r#12","source":"github","title":"Issue #12 flaky retry","url":"https://github.com/o/r/issues/12"}],
}
THREAD = {
 "key":"github:o/r#12","source":"github","title":"Issue #12 flaky retry","stage":"subscribed",
 "lastAt":"2026-11-04T10:12:00.000Z","createdAt":"2026-11-03T08:00:00.000Z","subscriptions":1,"pending":0,
 # The bundle sends the full ticket row here, not just id/title/state: /thread shows the
 # subscribed tickets with their stage, note and timestamp.
 "tickets":[dict(ROW)],
 "latest":{"actor":{"source":"github","id":"alice","name":{"zh":"爱丽丝","en":"Alice"}},"human":True,"at":"2026-11-04T10:12:00.000Z","preview":"What about option B?"},
 "url":"https://github.com/o/r/issues/12",
}
DECISION = {"id":"d-1","kind":"accept","outcome":"accept","reason":[{"code":"addressed"}],"at":"2026-11-04T09:00:30.000Z","strength":"hard","ticketId":ROW["id"],"triggerIds":["tr-1"],"thread":"github:o/r#12","threadTitle":"Issue #12 flaky retry","preview":"@agent please fix this","dismissed":False}
# A greeting the agent declined: the console prints this as `◇ agent · not taken`.
IGNORED = {"id":"d-2","kind":"ignore","outcome":"ignore","reason":[{"code":"said","values":{"text":"打招呼，没有具体改动请求"}}],"at":"2026-11-04T10:20:00.000Z","strength":"explicit","triggerIds":["tr-2"],"thread":"local:t-demo","threadTitle":"hi","preview":"hi","dismissed":False}
DETAIL = dict(ROW)
DETAIL.update({
 "mandate":{"actor":{"source":"github","id":"alice","name":{"zh":"爱丽丝","en":"Alice"}},"strength":"hard"},
 "basis":{"facts":[{"name":"addressed","value":"mention"},{"name":"sender","value":"alice"}],"legacy":False},
 "paused":False,
 "channels":[{"source":"github","url":"https://github.com/o/r/issues/12","label":"Issue #12"}],
 "decision":DECISION,
 "assignments":[{"id":"a-1","state":"running","startedAt":"2026-11-04T09:01:00.000Z","ref":"dsh-superagent-t-7f3a","report":{"state":"blocked","at":"2026-11-04T10:10:00.000Z","summary":"Two options: A retries the call, B restructures the queue.","question":"Which do you prefer?"}}],
 "inputs":[{"id":"i-1","kind":"trigger","delivered":True,"trigger":{"id":"tr-1","actor":{"source":"github","id":"alice","name":{"zh":"爱丽丝","en":"Alice"}},"human":True,"preview":"What about option B?","thread":"github:o/r#12"}}],
 "threads":[THREAD],
})

ROUTES = {
 "/orchestrator/api/state": {"paused":False,"masterId":"orchestrator-abc","health":{"runtime":{"state":"ready","detail":""},"github":{"state":"ready","detail":"last poll 20s ago"},"local":{"state":"ready","detail":"Waiting for an instruction"},"evals":{"state":"ready","detail":"no unavailable Eval"}},"load":{"running":1,"queued":0,"maxWorkers":4},"setup":{"problem":None,"accounts":{"repository":"o/r","agent":"agent-bot"}}},
 "/orchestrator/api/overview": {"problems":[{"kind":"source-error","source":{"id":"feishu","title":"Feishu","health":{"state":"error","detail":"bot CLI not configured"}}}],"person":[ROW],"executor":{"running":1,"queued":0,"maxWorkers":4,"idle":2},"flow":{"received":12,"filtered":3,"triggers":9,"judged":8,"accepted":2,"evaluating":1,"dropped":0},"policyVersion":"8f3a2c9d1e4b"},
 "/orchestrator/api/tickets": {"items":[ROW],"total":1,"next_offset":None},
 "/orchestrator/api/threads": {"items":[THREAD],"total":1,"next_offset":None},
 "/orchestrator/api/decisions": {"items":[DECISION,IGNORED],"next_offset":None},
 "/orchestrator/api/sources": [{"id":"github","kind":"github","title":"GitHub","state":"ready","enabled":True,"installed":True,"health":[{"state":"ready","detail":"ok"}],"stats":{"received":9,"filtered":2,"lastAt":"2026-11-04T10:11:00.000Z"}},{"id":"feishu","kind":"feishu","title":"Feishu","state":"error","enabled":False,"installed":True,"health":[{"state":"error","detail":"bot CLI not configured"}]}],
 "/orchestrator/api/search": {"tickets":[ROW],"threads":[THREAD],"items":[]},
}

DENIED = b'{"error":"Same-origin management request required"}'

class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def guard(self):
        # `sameOrigin()` pins the Host and, when present, the Origin, not just the request header.
        host = self.headers.get("Host")
        port = self.server.server_address[1]
        origin = self.headers.get("Origin")
        if (host not in (f"127.0.0.1:{port}", f"localhost:{port}")
                or self.headers.get("x-orchestrator-request") != "1"
                or (origin is not None and origin != f"http://{host}")):
            self.send_response(403); self.send_header("content-type","application/json"); self.send_header("content-length",str(len(DENIED))); self.end_headers(); self.wfile.write(DENIED); return False
        return True
    def do_GET(self):
        if not self.guard(): return
        path = urlparse(self.path).path
        if path.startswith("/orchestrator/api/tickets/") and path != "/orchestrator/api/tickets":
            body = DETAIL
        elif path.startswith("/orchestrator/api/threads/"):
            body = dict(THREAD); body.update({"timeline":[{"id":"tr-1","source":"github","actor":{"source":"github","id":"alice","name":{"zh":"爱丽丝","en":"Alice"}},"human":True,"at":"2026-11-04T09:00:00.000Z","preview":"@agent please fix this","text":"@agent please fix this"}],"decisions":[DECISION],"correlated":[],"context":[],"earlier":None})
        else:
            body = ROUTES.get(path)
        if body is None:
            self.send_response(404); self.end_headers(); self.wfile.write(b'{"error":"not found"}'); return
        raw = json.dumps(body, ensure_ascii=False).encode()
        self.send_response(200); self.send_header("content-type","application/json"); self.send_header("content-length",str(len(raw))); self.end_headers(); self.wfile.write(raw)
    def do_POST(self):
        if not self.guard(): return
        raw = b'{"ok":true}'
        self.send_response(200); self.send_header("content-type","application/json"); self.send_header("content-length",str(len(raw))); self.end_headers(); self.wfile.write(raw)

ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
