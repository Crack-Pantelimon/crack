ERROR:    unscripted-chat: exchange failed for 1784751363167
Traceback (most recent call last):
  File "/workspace/.pi/crack/server/src/crack_server/steprun.py", line 266, in record_chat_errors
    yield
  File "/workspace/.pi/crack/server/src/crack_server/chat_engine.py", line 116, in run_exchange
    reason = await _run_prewalk_loop(
             ^^^^^^^^^^^^^^^^^^^^^^^^
    ...<8 lines>...
    )
    ^
  File "/workspace/.pi/crack/server/src/crack_server/chat_engine.py", line 195, in _run_prewalk_loop
    reason = await pi_runner.arun_agent_hop(
             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    ...<17 lines>...
    )
    ^
  File "/workspace/.pi/crack/server/src/crack_server/pi_proc.py", line 388, in arun_agent_hop
    return await pi_rpc.arun_agent_hop_rpc(
           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    ...<23 lines>...
    )
    ^
  File "/workspace/.pi/crack/server/src/crack_server/pi_rpc.py", line 572, in arun_agent_hop_rpc
    result = await _run_single_rpc_attempt(
             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    ...<19 lines>...
    )
    ^
  File "/workspace/.pi/crack/server/src/crack_server/pi_rpc.py", line 323, in _run_single_rpc_attempt
    await _send_line(
    ...<2 lines>...
    )
  File "/workspace/.pi/crack/server/src/crack_server/pi_rpc.py", line 120, in _send_line
    proc.stdin.write((json.dumps(payload) + "\n").encode())
    ~~~~~~~~~~~~~~~~^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "/usr/lib/python3.13/asyncio/streams.py", line 340, in write
    self._transport.write(data)
    ~~~~~~~~~~~~~~~~~~~~~^^^^^^
  File "uvloop/handles/stream.pyx", line 675, in uvloop.loop.UVStream.write
  File "uvloop/handles/handle.pyx", line 159, in uvloop.loop.UVHandle._ensure_alive
RuntimeError: unable to perform operation on <WriteUnixTransport closed=True reading=False 0x7f69e6b1f440>; the handler is closed