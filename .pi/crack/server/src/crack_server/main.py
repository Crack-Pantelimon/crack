import os
from pathlib import Path

import uvicorn

# from crack_server.app import app


def main() -> None:
    host = os.environ.get("CRACK_PI_HOST", "127.0.0.1")
    port = int(os.environ.get("CRACK_PI_PORT", "9847"))
    # Watch only the package source: state writes under the data dirs
    # (run.json, queue files, reports) must not trigger reload storms.
    src_dir = Path(__file__).resolve().parent
    uvicorn.run(
        "crack_server.app:app",
        host=host,
        port=port,
        log_level="info",
        reload=True,
        reload_dirs=[str(src_dir)],
    )


if __name__ == "__main__":
    main()
