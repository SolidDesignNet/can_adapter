use std::fmt::Display;

use anyhow::*;

#[derive(Debug)]
pub struct Rp1210Dev {
    pub id: i16,
    pub name: String,
    pub description: String,
}
#[derive(Debug)]
pub struct Rp1210Prod {
    pub id: String,
    pub description: String,
    pub devices: Vec<Rp1210Dev>,
}

impl Display for Rp1210Dev {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}:{}", self.id, self.name, self.description)
    }
}
impl Display for Rp1210Prod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{} {}", self.id, self.description)?;
        for d in &self.devices {
            writeln!(f, "{}", d)?;
        }
        std::fmt::Result::Ok(())
    }
}

pub fn list_all_products() -> Result<Vec<Rp1210Prod>> {
    let start = std::time::Instant::now();
    let load_from_file = ini::Ini::load_from_file("c:\\Windows\\RP121032.ini");
    if load_from_file.is_err() {
        // don't fail on linux
        return Ok(Vec::new());
    }
    let rtn = Ok(load_from_file?
        .get_from(Some("RP1210Support"), "APIImplementations")
        .unwrap_or("")
        .split(',')
        .map(|s| {
            let (description, devices) = list_devices_for_prod(s).unwrap();
            Rp1210Prod {
                id: s.to_string(),
                description: description.to_string(),
                devices,
            }
        })
        .collect());
    println!("RP1210 INI parsing in {} ms", start.elapsed().as_millis());
    rtn
}

fn list_devices_for_prod(id: &str) -> Result<(String, Vec<Rp1210Dev>)> {
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
            section.unwrap().starts_with("DeviceInformation")
                && j1939_devices.contains(&properties.get("DeviceID").unwrap_or("X"))
        })
        .map(|(_, properties)| Rp1210Dev {
            id: properties.get("DeviceID").unwrap_or("0").parse().unwrap(),
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
        .unwrap()
        .get("Name")
        .unwrap()
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
