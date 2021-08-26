import argparse
import socket

import serial

PACKET_SIZE = 14

def SerialPort(s: str) -> serial.Serial:
    return serial.serial_for_url(s)


parser = argparse.ArgumentParser('dump')
parser.add_argument('serial_port', metavar='serial-port', type=SerialPort, help='Serial port device or URL')
parser.add_argument('--port', type=int, default=45654, help='UDP port')

args = parser.parse_args()

udp_sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
udp_sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)

s_port = args.serial_port

while True:
    buf = s_port.read(PACKET_SIZE)

    discard = 0
    while buf[0] != 0x32 and buf[-1] != 0x34:
        buf = buf[1:] + s_port.read(1)
        discard += 1
        assert len(buf) == PACKET_SIZE

    if discard != 0:
        print(f'Discarded {discard} byte(s) to re-sync.')

    udp_sock.sendto(buf, ('<broadcast>', args.port))


