#!/usr/bin/env python3
"""Minimal IPv4 TCP forwarder: 0.0.0.0:<listen> -> <dst_host>:<dst_port>.

supergateway (used to bridge the stdio MCP servers to HTTP/SSE) calls
``app.listen(port)`` with no host, which on this image binds IPv6 ``::`` only.
Docker's userland proxy forwards published ports over IPv4, so a ``::``-only
listener is unreachable from the host. We therefore run supergateway on an
internal loopback port and expose it on all IPv4 interfaces through this
forwarder, which the Docker port publish can reach. No third-party deps.

Usage: tcp_forward.py <listen_port> <dst_host> <dst_port>
"""

import asyncio
import logging
import socket
import sys

logging.basicConfig(level=logging.INFO, format="%(asctime)s tcp_forward: %(message)s")
log = logging.getLogger(__name__)


async def _pipe(reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
    try:
        while data := await reader.read(65536):
            writer.write(data)
            await writer.drain()
    except (ConnectionError, OSError):
        pass
    finally:
        try:
            writer.close()
        except OSError:
            pass


def _nodelay(writer: asyncio.StreamWriter) -> None:
    # SSE first events are tiny; without TCP_NODELAY, Nagle can hold that packet
    # so the streamed endpoint event never reaches the client through this hop.
    sock = writer.get_extra_info("socket")
    if sock is not None:
        try:
            sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        except OSError:
            pass


async def _handle(local_reader, local_writer, dst_host: str, dst_port: int) -> None:
    try:
        remote_reader, remote_writer = await asyncio.open_connection(dst_host, dst_port)
    except OSError as e:
        log.warning("upstream %s:%s unavailable: %s", dst_host, dst_port, e)
        local_writer.close()
        return
    _nodelay(local_writer)
    _nodelay(remote_writer)
    await asyncio.gather(
        _pipe(local_reader, remote_writer),
        _pipe(remote_reader, local_writer),
    )


async def _main() -> None:
    listen_port = int(sys.argv[1])
    dst_host = sys.argv[2]
    dst_port = int(sys.argv[3])
    server = await asyncio.start_server(
        lambda r, w: _handle(r, w, dst_host, dst_port), "0.0.0.0", listen_port
    )
    log.info("forwarding 0.0.0.0:%d -> %s:%d", listen_port, dst_host, dst_port)
    async with server:
        await server.serve_forever()


if __name__ == "__main__":
    asyncio.run(_main())
