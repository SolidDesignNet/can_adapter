use std::fmt::Display;

use anyhow::*;

#[derive(Debug)]
pub struct Rp1210Device {
    pub id: i16,
    pub name: String,
    pub description: String,
}
#[derive(Debug)]
pub struct Rp1210Product {
    pub id: String,
    pub description: String,
    pub devices: Vec<Rp1210Device>,
}

impl Display for Rp1210Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}:{}", self.id, self.name, self.description)
    }
}
impl Display for Rp1210Product {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{} {}", self.id, self.description)?;
        for d in &self.devices {
            writeln!(f, "{}", d)?;
        }
        std::fmt::Result::Ok(())
    }
}

pub fn list_all_products() -> Result<Vec<Rp1210Product>> {
    let start = std::time::Instant::now();
    let load_from_file = ini::Ini::load_from_file("c:\\Windows\\RP121032.ini");
    if load_from_file.is_err() {
        // don't fail on linux
        return Ok(vec![
            Rp1210Product {
                id: "SIM".to_string(),
                description: "Simulated Adapter 1".to_string(),
                devices: vec![Rp1210Device {
                    id: 1,
                    name: "SIM".to_string(),
                    description: "Simulated Device".to_string(),
                }],
            },
            Rp1210Product {
                id: "SIM".to_string(),
                description: "Simulated Adapter 2".to_string(),
                devices: vec![Rp1210Device {
                    id: 2,
                    name: "SIM".to_string(),
                    description: "Simulated Device 2".to_string(),
                }],
            },
            Rp1210Product {
                id: "SIM".to_string(),
                description: "Simulated Adapter 3".to_string(),
                devices: vec![Rp1210Device {
                    id: 3,
                    name: "SIM".to_string(),
                    description: "Simulated Device 3".to_string(),
                }],
            },
        ]);
    }
    let rtn = Ok(load_from_file?
        .get_from(Some("RP1210Support"), "APIImplementations")
        .unwrap_or("")
        .split(',')
        .map(|s| {
            let (description, devices) = list_devices_for_prod(s).unwrap_or_default();
            Rp1210Product {
                id: s.to_string(),
                description: description.to_string(),
                devices,
            }
        })
        .collect());
    println!("RP1210 INI parsing in {} ms", start.elapsed().as_millis());
    rtn
}

fn list_devices_for_prod(id: &str) -> Result<(String, Vec<Rp1210Device>)> {
    let start = std::time::Instant::now();
    let ini = ini::Ini::load_from_file(&format!("c:\\Windows\\{}.ini", id))?;

    // find device IDs for J1939
    let j1939_devices: Vec<&str> = ini
        .iter()
        // find J1939 protocol description
        .filter(|(section, properties)| {
            section.unwrap_or("").starts_with("ProtocolInformation")
                && properties.get("ProtocolString") == Some("J1939")
        })
        // which device ids support J1939?
        .flat_map(|(_, properties)| {
            properties
                .get("Devices")
                .map_or(vec![], |s| s.split(',').collect())
        })
        .collect();

    // find the specified devices
    let rtn = ini
        .iter()
        .filter(|(section, properties)| {
            section
                .map(|n| n.starts_with("DeviceInformation"))
                .unwrap_or(false)
                && properties
                    .get("DeviceID")
                    .map(|id| j1939_devices.contains(&id))
                    .unwrap_or(false)
        })
        .map(|(_, properties)| Rp1210Device {
            id: properties.get("DeviceID").unwrap_or("0").parse().unwrap_or(-1),
            name: properties
                .get("DeviceName")
                .unwrap_or("Unknown")
                .to_string(),
            description: properties
                .get("DeviceDescription")
                .unwrap_or("Unknown")
                .to_string(),
        })
        .collect();
    println!("  {}.ini parsing in {} ms", id, start.elapsed().as_millis());
    let description = ini
        .section(Some("VendorInformation"))
        .and_then(|s|s.get("Name"))
        .unwrap_or_default()
        .to_string();
    Ok((description, rtn))
}

#[allow(dead_code)]
pub fn time_stamp_weight(id: &str) -> Result<f64> {
    let ini = ini::Ini::load_from_file(&format!("c:\\Windows\\{}.ini", id))?;
    Ok(ini
        .get_from_or::<&str>(Some("VendorInformation"), "TimeStampWeight", "1")
        .parse()?)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple() -> Result<(), Error> {
        list_all_products()?;
        Ok(())
    }
}
