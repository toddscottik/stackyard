"""Run the FastAPI starter and reload its app.py on save."""

from pathlib import Path
import subprocess
import sys
import time


def revision(source):
    try:
        return source.stat().st_mtime_ns
    except FileNotFoundError:
        # Editors can briefly remove a file while replacing it.
        return None


def main():
    source = Path(__file__).with_name("app.py")
    command = [sys.executable, "-B", "-m", "uvicorn", "app:app", *sys.argv[1:]]
    while True:
        current = revision(source)
        server = subprocess.Popen(command, cwd=source.parent)
        try:
            while True:
                status = server.poll()
                if status is not None:
                    return status
                updated = revision(source)
                if updated is not None and updated != current:
                    print("app.py changed. Restarting Uvicorn.", flush=True)
                    break
                time.sleep(0.25)
        finally:
            # Windows console signals cannot reliably target this child.
            if server.poll() is None:
                server.terminate()
                try:
                    server.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.wait()


if __name__ == "__main__":
    raise SystemExit(main())
