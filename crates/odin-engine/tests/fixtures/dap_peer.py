"""DAP protocol peer fixture. It never launches a debuggee."""
import json
import sys
from pathlib import Path

def read_message():
    headers = {}
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        if line in (b'\r\n', b'\n'):
            break
        key, value = line.decode('ascii').split(':', 1)
        headers[key.lower()] = value.strip()
    return json.loads(sys.stdin.buffer.read(int(headers['content-length'])))

sequence = 1
def send(message):
    global sequence
    message['seq'] = sequence
    sequence += 1
    data = json.dumps(message).encode('utf-8')
    sys.stdout.buffer.write(f'Content-Length: {len(data)}\r\n\r\n'.encode('ascii') + data)
    sys.stdout.buffer.flush()

launched = False
while (message := read_message()) is not None:
    if '--record' in sys.argv:
        with Path(sys.argv[sys.argv.index('--record') + 1]).open('a', encoding='utf-8') as log:
            log.write(json.dumps(message) + '\n')
    command = message['command']
    body = {}
    if command == 'initialize':
        body = {'supportsConfigurationDoneRequest': True, 'supportsTerminateRequest': True}
    elif command in ('launch', 'attach'):
        launched = True
    elif command == 'threads':
        body = {'threads': [{'id': 1, 'name': 'fixture'}]}
    elif command == 'evaluate':
        body = {'result': '42', 'variablesReference': 0}
    elif command == 'setBreakpoints':
        body = {'breakpoints': []}
    send({'type': 'response', 'request_seq': message['seq'], 'command': command, 'success': True, 'body': body})
    if command == 'initialize':
        send({'type': 'event', 'event': 'initialized', 'body': {}})
    elif command == 'configurationDone' and launched:
        send({'type': 'event', 'event': 'stopped', 'body': {'reason': 'entry', 'threadId': 1, 'allThreadsStopped': True}})
    elif command in ('disconnect', 'terminate'):
        send({'type': 'event', 'event': 'terminated', 'body': {}})
        break
