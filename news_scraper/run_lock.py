import errno
import json
import os
import socket
from dataclasses import asdict, dataclass
from datetime import datetime
from pathlib import Path
from typing import Any

class RunAlreadyActiveError(RuntimeError):
    def __init__(self, metadata: dict[str, Any] | None = None):
        self.metadata = metadata or {}
        detail = ""
        if self.metadata.get("pid"):
            detail = " (PID {})".format(self.metadata["pid"])
        super().__init__("已有另一個新聞整理程序正在執行{}。".format(detail))


@dataclass(frozen=True)
class RunLockMetadata:
    pid: int
    hostname: str
    started_at: str
    mode: str


def _process_is_running(pid: int) -> bool:
    if pid <= 0:
        return False
    try:
        os.kill(pid, 0)
    except OSError as exc:
        return exc.errno == errno.EPERM
    return True


def _read_metadata(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}
    return value if isinstance(value, dict) else {}


def _is_stale(metadata: dict[str, Any]) -> bool:
    if metadata.get("hostname") != socket.gethostname():
        return False
    try:
        pid = int(metadata.get("pid", 0))
    except (TypeError, ValueError):
        return False
    return not _process_is_running(pid)


class RunLock:
    def __init__(self, path: str | Path, mode: str):
        self.path = Path(path)
        self.mode = mode
        self._acquired = False

    def acquire(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        metadata = RunLockMetadata(
            pid=os.getpid(),
            hostname=socket.gethostname(),
            started_at=datetime.now().astimezone().isoformat(timespec="seconds"),
            mode=self.mode,
        )
        payload = json.dumps(asdict(metadata), ensure_ascii=False, indent=2)

        for _ in range(2):
            try:
                descriptor = os.open(self.path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
            except FileExistsError:
                existing = _read_metadata(self.path)
                if _is_stale(existing):
                    try:
                        self.path.unlink()
                    except FileNotFoundError:
                        pass
                    continue
                raise RunAlreadyActiveError(existing)
            else:
                with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
                    stream.write(payload)
                    stream.flush()
                    os.fsync(stream.fileno())
                self._acquired = True
                return
        raise RunAlreadyActiveError(_read_metadata(self.path))

    def release(self) -> None:
        if not self._acquired:
            return
        try:
            current = _read_metadata(self.path)
            if int(current.get("pid", -1)) == os.getpid():
                self.path.unlink(missing_ok=True)
        finally:
            self._acquired = False

    def __enter__(self):
        self.acquire()
        return self

    def __exit__(self, exc_type, exc, traceback):
        self.release()
        return False
