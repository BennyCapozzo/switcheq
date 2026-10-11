//! Ricerca delle cuffie accoppiate tramite WinRT (Windows.Devices.Enumeration),
//! stato di connessione e livello batteria esposto da Windows.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Headset {
    pub id: String,
    pub name: String,
    pub address: Option<String>,
    pub connected: bool,
    /// Percentuale batteria se Windows la espone (profilo HFP), altrimenti `None`.
    pub battery: Option<u8>,
}

/// Confronto case-insensitive "contiene uno dei pattern".
pub fn name_matches(name: &str, patterns: &[String]) -> bool {
    let name = name.to_lowercase();
    patterns
        .iter()
        .map(|p| p.trim().to_lowercase())
        .any(|p| !p.is_empty() && name.contains(&p))
}

#[cfg(windows)]
mod imp {
    use super::Headset;
    use windows::core::{Interface, GUID, HSTRING};
    use windows::Devices::Bluetooth::BluetoothDevice;
    use windows::Devices::Enumeration::{DeviceInformation, DeviceInformationKind};
    use windows_collections::IIterable;
    use windows::Foundation::IReference;

    const P_CONNECTED: &str = "System.Devices.Aep.IsConnected";
    const P_CONTAINER: &str = "System.Devices.Aep.ContainerId";
    const P_ADDRESS: &str = "System.Devices.Aep.DeviceAddress";
    /// DEVPKEY_Bluetooth_Battery: percentuale riportata dalle cuffie via HFP.
    const P_BATTERY: &str = "{104EA319-6EE2-4701-BD47-8DDBF425BBE5} 2";

    fn props(names: &[&str]) -> windows::core::Result<IIterable<HSTRING>> {
        let v: Vec<HSTRING> = names.iter().map(|s| HSTRING::from(*s)).collect();
        Ok(IIterable::from(v))
    }

    fn lookup<T>(di: &DeviceInformation, key: &str) -> Option<T>
    where
        T: windows::core::RuntimeType + 'static,
        IReference<T>: Interface,
    {
        let map = di.Properties().ok()?;
        let key = HSTRING::from(key);
        if !map.HasKey(&key).ok()? {
            return None;
        }
        map.Lookup(&key).ok()?.cast::<IReference<T>>().ok()?.Value().ok()
    }

    fn lookup_string(di: &DeviceInformation, key: &str) -> Option<String> {
        lookup::<HSTRING>(di, key).map(|s| s.to_string()).filter(|s| !s.is_empty())
    }

    pub fn battery_for_container(container: GUID) -> Option<u8> {
        let aqs = HSTRING::from(format!("System.Devices.ContainerId:=\"{{{container:?}}}\""));
        let list = DeviceInformation::FindAllAsyncWithKindAqsFilterAndAdditionalProperties(
            &aqs,
            &props(&[P_BATTERY]).ok()?,
            DeviceInformationKind::Device,
        )
        .ok()?
        .join()
        .ok()?;
        list.into_iter()
            .filter_map(|d| lookup::<u8>(&d, P_BATTERY))
            .filter(|&b| b <= 100)
            .max()
    }

    pub fn find_where(keep: impl Fn(&str) -> bool, with_battery: bool) -> Result<Vec<Headset>, String> {
        let selector = BluetoothDevice::GetDeviceSelectorFromPairingState(true).map_err(|e| e.message())?;
        let list = DeviceInformation::FindAllAsyncAqsFilterAndAdditionalProperties(
            &selector,
            &props(&[P_CONNECTED, P_CONTAINER, P_ADDRESS]).map_err(|e| e.message())?,
        )
        .and_then(|op| op.join())
        .map_err(|e| crate::t!("err.bt.enum", error = e.message()))?;

        let mut out = Vec::new();
        for di in list {
            let name = di.Name().map(|n| n.to_string()).unwrap_or_default();
            if !keep(&name) {
                continue;
            }
            let connected = lookup::<bool>(&di, P_CONNECTED).unwrap_or(false);
            let battery = if with_battery && connected {
                lookup::<GUID>(&di, P_CONTAINER).and_then(battery_for_container)
            } else {
                None
            };
            out.push(Headset {
                id: di.Id().map(|s| s.to_string()).unwrap_or_default(),
                name,
                address: lookup_string(&di, P_ADDRESS),
                connected,
                battery,
            });
        }
        // Le cuffie connesse per prime.
        out.sort_by(|a, b| b.connected.cmp(&a.connected).then_with(|| a.name.cmp(&b.name)));
        Ok(out)
    }
}

#[cfg(windows)]
use imp::find_where;

#[cfg(not(windows))]
fn find_where(_keep: impl Fn(&str) -> bool, _with_battery: bool) -> Result<Vec<Headset>, String> {
    Ok(Vec::new())
}

/// Dispositivi accoppiati i cui nomi contengono uno dei pattern.
pub fn find(patterns: &[String], with_battery: bool) -> Result<Vec<Headset>, String> {
    find_where(|n| name_matches(n, patterns), with_battery)
}

/// Nomi di tutti i dispositivi Bluetooth accoppiati (senza batteria).
pub fn paired_names() -> Vec<String> {
    find_where(|_| true, false).map(|v| v.into_iter().map(|h| h.name).collect()).unwrap_or_default()
}

/// Nome Bluetooth accoppiato che compare nel nome di un endpoint audio
/// ("Cuffie (Acme One Stereo)" → "Acme One"); il più lungo se più d'uno.
/// `None` se l'endpoint non è di un dispositivo Bluetooth accoppiato (cablato).
pub fn paired_name_in(endpoint_name: &str, paired: &[String]) -> Option<String> {
    let endpoint = endpoint_name.to_lowercase();
    paired
        .iter()
        .map(|n| n.trim())
        .filter(|n| n.chars().count() >= 3 && endpoint.contains(&n.to_lowercase()))
        .max_by_key(|n| n.chars().count())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::name_matches;

    #[test]
    fn matches_case_insensitive_substring() {
        let p = vec!["Acme One".to_string(), "acme-1".to_string(), " ".to_string()];
        assert!(name_matches("Acme One Pro", &p));
        assert!(name_matches("ACME ONE", &p));
        assert!(!name_matches("WH-1000XM5", &p));
        assert!(!name_matches("Qualsiasi", &[String::new()]));
    }

    #[test]
    fn endpoint_names_map_to_the_paired_device() {
        use super::paired_name_in;
        let paired = vec!["Acme One".to_string(), "Acme One Pro".to_string(), "X".to_string(), " ".to_string()];
        assert_eq!(paired_name_in("Cuffie (Acme One Stereo)", &paired).as_deref(), Some("Acme One"));
        assert_eq!(paired_name_in("Headset (ACME ONE PRO Hands-Free)", &paired).as_deref(), Some("Acme One Pro"));
        assert_eq!(paired_name_in("Altoparlanti (Realtek High Definition Audio)", &paired), None);
        assert_eq!(paired_name_in("Cuffie (USB Audio X)", &paired), None, "nomi troppo corti ignorati");
    }

    /// Smoke test sull'hardware reale: `cargo test -- --ignored --nocapture`.
    #[test]
    #[ignore = "richiede Windows con Bluetooth; stampa dispositivi ed endpoint"]
    fn smoke_enumerate_real_devices() {
        let any: Vec<String> = "aeiou".chars().map(String::from).collect();
        for h in super::find(&any, true).expect("enumerazione Bluetooth") {
            println!("BT  {h:?}");
        }
        for e in crate::audio::endpoints(&any).expect("endpoint audio") {
            println!("EP  {e:?}");
        }
        println!("KS  filtri raggiungibili: {}", crate::audio::ks_filter_count(&any));
    }
}
