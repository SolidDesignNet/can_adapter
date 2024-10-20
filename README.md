### RP1210 API

Brokers packets from a queue to and from the attached RP1210 adapter.  Includes:
1. RP1210 calls
2. RP1210 .INI file parsing
3. queue that supports multiple listeners
4. packet that encapsulates the byte[]
5. simulator for development on machines that don't support RP1210

### Usage for command line J1939 logger
```
Usage: Usage: rp1210 [OPTIONS] --adapter <ADAPTER> --device <DEVICE>

RP1210 Devices:
  PEAKRP32 PEAK-System PCAN Adapter
    --adapter PCAN-USB --device 1: PEAK-System CAN Adapter (USB, 1 Channel)

Options:
  -D, --adapter <ADAPTER>
          RP1210 Adapter Identifier
  -d, --device <DEVICE>
          RP1210 Device ID
  -C, --connection-string <CONNECTION_STRING>
          RP1210 Connection String [default: J1939:Baud=Auto]
  -a, --sa <SOURCE_ADDRESS>
          RP1210 Adapter Address (used for packets send and transport protocol) [default: F9]
  -v, --verbose

      --app-packetize

  -h, --help
          Print help
```
