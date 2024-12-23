# CAN Adapter API

*J2534 is a work in progress, but RP1210 works.*

Brokers packets from a queue to and from the attached RP1210 adapter.  Includes:
1. RP1210 calls
2. RP1210 .INI file parsing
3. Bus that supports multiple listeners
4. packet that encapsulates the byte[]
5. simulator for development on machines that don't support RP1210

# Usage for command line J1939 logger
```
Usage: Usage: logger [OPTIONS] --adapter <ADAPTER> --device <DEVICE>

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

# API
Example:
```rust
    // request VIN from ECM
    // start collecting packets
    let mut packets = rp1210.iter_for(Duration::from_secs(5));
    // send request for VIN
    rp1210.push(J1939Packet::new(1, 0x18EA00F9, &[0xEC, 0xFE, 0x00]));
    // filter for ECM result
    packets
        .find(|p| p.pgn() == 0xFEEC && p.source() == 0)
        // log the VINs
        .map(|p| {
            print!(
                "ECM {:02X} VIN: {}\n{}",
                p.source(),
                String::from_utf8(p.data.clone()).unwrap(),
                p
            )
        });
```

# Applications
When combined with DBC or J1939DA parsing, this becomes a light weight CAN logger.  See https://github.com/SolidDesignNet/j1939logger.