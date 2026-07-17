import os

import uvicorn

# from crack_server.app import app


def main() -> None:
    host = os.environ.get("CRACK_PI_HOST", "0.0.0.0")
    port = int(os.environ.get("CRACK_PI_PORT", "9847"))
    uvicorn.run("crack_server.app:app", host=host, port=port, log_level="info", reload=True)


if __name__ == "__main__":
    main()
