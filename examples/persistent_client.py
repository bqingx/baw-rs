#!/usr/bin/env python3
"""Same-user JSONL resident client. Never retries accepted/uncertain requests.

Import PersistentClient and call request(op=..., name=..., ...). Running this
example alone sends only ping; it cannot submit a transaction.
"""
import argparse
import json
import os
import socket
import stat
import struct
import uuid
from pathlib import Path


class PersistentClient:
    MAX_FRAME = 65536
    MAX_RESPONSE = 8 * 1024 * 1024

    def __init__(self, path, timeout=400):
        path = Path(path)
        parent, node = path.parent.lstat(), path.lstat()
        if (not stat.S_ISDIR(parent.st_mode) or parent.st_uid != os.geteuid()
                or stat.S_IMODE(parent.st_mode) != 0o700
                or not stat.S_ISSOCK(node.st_mode) or node.st_uid != os.geteuid()
                or stat.S_IMODE(node.st_mode) != 0o600):
            raise PermissionError("socket and parent must be owned/private (0600/0700)")
        self.socket = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.socket.settimeout(timeout)
        try:
            self.socket.connect(str(path))
            _, uid, _ = struct.unpack("3i", self.socket.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
            if uid != os.geteuid():
                raise PermissionError("resident uid mismatch")
            self.reader = self.socket.makefile("rb")
        except BaseException:
            self.socket.close()
            raise

    def request(self, **fields):
        fields.setdefault("id", uuid.uuid4().hex)
        frame = json.dumps(fields, separators=(",", ":")).encode()
        if len(frame) > self.MAX_FRAME:
            raise ValueError("request exceeds 64KiB")
        try:
            self.socket.sendall(frame + b"\n")
            response = self.reader.readline(self.MAX_RESPONSE + 2)
            if not response.endswith(b"\n") or len(response) > self.MAX_RESPONSE + 1:
                raise RuntimeError("incomplete/oversized response; outcome UNKNOWN; never retry")
            result = json.loads(response)
            if result.get("id") != fields["id"]:
                raise RuntimeError("correlation mismatch; outcome UNKNOWN; never retry")
            return result
        except BaseException:
            self.close()
            raise

    def prepare(self, name, operation_id, deadline_ms, call, policy=None):
        fields = dict(op="prepare", name=name, operation_id=operation_id,
                      deadline_ms=deadline_ms, call=call)
        if policy is not None:
            fields["policy"] = policy
        return self.request(**fields)

    def trigger(self, name, operation_id, *, confirm=False):
        # No automatic confirmation and no automatic resubmission, even on timeout.
        return self.request(op="trigger", name=name, operation_id=operation_id, confirm=confirm)

    def disarm(self, name, operation_id):
        return self.request(op="disarm", name=name, operation_id=operation_id)

    def close(self):
        if hasattr(self, "reader"):
            self.reader.close()
        self.socket.close()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("socket")
    args = parser.parse_args()
    with PersistentClient(args.socket) as client:
        print(json.dumps(client.request(op="ping")))
