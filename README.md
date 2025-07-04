# CAN Adapter API

*J2534 is a work in progress, but RP1210, SLCAN, and SocketCAN work.*

Brokers packets from a queue to and from the attached adapter.  Includes:
1. Discovering configured adapters
2. Bus that supports multiple listeners
3. packet that encapsulates the byte[] (called J1939Packet.  Needs to be enhanced to support raw CAN, 11 bit CAN, etc.)
4. simulator for unit testing

# Usage for command line J1939 logger
```
CAN tool

Usage: logger [OPTIONS] <CONNECTION> <COMMAND>

Commands:
  log        Dump Vector ASC compatible log to stdout
  server     Used for testing.  Requires another instance to send or ping this source address
  ping       Latency test. Ping [da] with as many requests as it will respond to
  bandwidth  Bandwidth test.  Send as much data to [da] with as many requests as it will respond to
  send       Send arbitrary CAN message
  vin        Read the VIN
  uds        Common UDS requests. See "uds --help" for more
  j1939      Common J1939 requests. See "j1939 --help" for more
  help       Print this message or the help of the given subcommand(s)

Arguments:
  <CONNECTION>  For a list of possible connections, "cancan list log".  Available connection strings will vary depending on the machine

Options:
  -s, --sa <SOURCE_ADDRESS>       Adapter Address (used for packets send and transport protocol) [default: 0xF9]
  -d, --da <DESTINATION_ADDRESS>  Adapter Address (used for packets send and transport protocol) [default: 0xFF]
  -t, --timeout <TIMEOUT>         Timeout in ms [default: 2000]
  -v, --verbose                   
  -h, --help                      Print help
  -V, --version                   Print version

```
- `log` does what you would expect and prints all of the packets to stdout in a format simir to Vector's .ASC files.
- `server`, `ping`, and `bandwidth` are used to performance test adapters.
- `send` sends an arbitrary packet specified in a format similar to Vector's ASC file format.
- `vin` reads the VIN from address 0 and broadcast. It is used as a demonstration.
- `uds` is intended to be a command line implementation of ISO14229. It currently supports ISO15765.
- `j1939` allows J1939 requests. It currently supports receiving J1939-21 transport protocol.  Sending transport protocol has not be validated beyond a self test.
- `--sa` and `--da` are to configure RP1210 adapters have have built in support for J1939-21 transport protocol.
# API
See main.rs implmentation for `fn vin(...)` https://github.com/SolidDesignNet/can_adapter/blob/main/src/main.rs#L357

# Applications
When combined with DBC or J1939DA parsing, this becomes a light weight CAN logger.  See https://github.com/SolidDesignNet/j1939logger.

# Note for Linux setup:
slcan setup:
```
chmod a+rw /dev/ttyACM0
```
then the logger can be used to log all J1939 requests and TP packets between F9 and 00:
```
logger 'slcan /dev/ttyACM0 500' log | egrep 'E[ABC]..(00|F9)'
```

PEAK
```
ip link set can0 name peak
ip link set peak type can bitrate 500000
ip link set peak up
```
