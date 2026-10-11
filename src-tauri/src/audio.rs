//! Endpoint audio (Core Audio): elenco delle uscite, cuffie Bluetooth, predefinito.
//!
//! - Connetti / disconnetti: WinRT non offre un'API per connettere un
//!   dispositivo audio Bluetooth Classic già accoppiato. Si usa lo stesso
//!   meccanismo del pulsante "Connetti" delle Impostazioni di Windows: la
//!   proprietà kernel-streaming `KSPROPSETID_BtAudio` (ONESHOT_RECONNECT /
//!   ONESHOT_DISCONNECT) sul filtro KS dell'endpoint audio. Se fallisce, il
//!   frontend propone di aprire le impostazioni Bluetooth.
//! - Dispositivo predefinito: interfaccia COM non documentata `IPolicyConfig`
//!   (usata da molti strumenti, stabile da Windows 7 a 11).

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Endpoint {
    pub id: String,
    pub name: String,
    pub render: bool,
    pub active: bool,
}

#[cfg(windows)]
mod imp {
    use super::Endpoint;
    use crate::bluetooth::name_matches;
    use std::ffi::c_void;
    use windows::core::{Interface, GUID, PCWSTR, PWSTR};
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows::Win32::Media::Audio::{
        eAll, eConsole, eMultimedia, eRender, IConnector, IDeviceTopology, IMMDevice,
        IMMDeviceEnumerator, IMMEndpoint, IPart, MMDeviceEnumerator, DEVICE_STATE, DEVICE_STATE_ACTIVE, DEVICE_STATE_UNPLUGGED,
    };
    use windows::Win32::Media::KernelStreaming::{IKsControl, KSIDENTIFIER, KSIDENTIFIER_0_0, KSPROPERTY_TYPE_GET};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
    };

    /// {7FA06C40-B8F6-4C7E-8556-E8C33A12E54D} da bthhfpddi.h / ksmedia.h.
    const KSPROPSETID_BTAUDIO: GUID = GUID::from_u128(0x7fa06c40_b8f6_4c7e_8556_e8c33a12e54d);
    const KSPROPERTY_ONESHOT_RECONNECT: u32 = 0;
    const KSPROPERTY_ONESHOT_DISCONNECT: u32 = 1;

    const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

    #[allow(non_snake_case)]
    mod policy {
        use std::ffi::c_void;
        use windows::core::{interface, IUnknown, IUnknown_Vtbl, HRESULT, PCWSTR};

        /// Vtable di IPolicyConfig (Windows 7+). Usiamo solo SetDefaultEndpoint:
        /// gli altri metodi servono solo a mantenere l'ordine della vtable.
        #[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
        pub unsafe trait IPolicyConfig: IUnknown {
            fn GetMixFormat(&self, a: PCWSTR, b: *mut *mut c_void) -> HRESULT;
            fn GetDeviceFormat(&self, a: PCWSTR, b: i32, c: *mut *mut c_void) -> HRESULT;
            fn ResetDeviceFormat(&self, a: PCWSTR) -> HRESULT;
            fn SetDeviceFormat(&self, a: PCWSTR, b: *mut c_void, c: *mut c_void) -> HRESULT;
            fn GetProcessingPeriod(&self, a: PCWSTR, b: i32, c: *mut i64, d: *mut i64) -> HRESULT;
            fn SetProcessingPeriod(&self, a: PCWSTR, b: *mut i64) -> HRESULT;
            fn GetShareMode(&self, a: PCWSTR, b: *mut c_void) -> HRESULT;
            fn SetShareMode(&self, a: PCWSTR, b: *mut c_void) -> HRESULT;
            fn GetPropertyValue(&self, a: PCWSTR, b: *const c_void, c: *mut c_void) -> HRESULT;
            fn SetPropertyValue(&self, a: PCWSTR, b: *const c_void, c: *mut c_void) -> HRESULT;
            fn SetDefaultEndpoint(&self, id: PCWSTR, role: i32) -> HRESULT;
            fn SetEndpointVisibility(&self, a: PCWSTR, b: i32) -> HRESULT;
        }

        /// I metodi generati dalla macro sono privati al modulo: wrapper pubblico.
        pub unsafe fn set_default_endpoint(pc: &IPolicyConfig, id: PCWSTR, role: i32) -> HRESULT {
            pc.SetDefaultEndpoint(id, role)
        }
    }
    use policy::IPolicyConfig;

    fn com_init() {
        // S_FALSE / RPC_E_CHANGED_MODE indicano COM già inizializzato: va bene.
        // SAFETY: chiamata standard senza puntatori.
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    }

    fn take_pwstr(p: PWSTR) -> String {
        // SAFETY: stringa allocata da COM con CoTaskMemAlloc, la liberiamo dopo la copia.
        unsafe {
            let s = p.to_string().unwrap_or_default();
            CoTaskMemFree(Some(p.0 as *const c_void));
            s
        }
    }

    fn enumerator() -> windows::core::Result<IMMDeviceEnumerator> {
        com_init();
        // SAFETY: creazione COM standard.
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
    }

    fn describe(dev: &IMMDevice) -> windows::core::Result<Endpoint> {
        // SAFETY: tutte chiamate COM su interfacce valide.
        unsafe {
            let id = take_pwstr(dev.GetId()?);
            let store = dev.OpenPropertyStore(STGM_READ)?;
            let name = store.GetValue(&PKEY_Device_FriendlyName)?.to_string();
            let render = dev.cast::<IMMEndpoint>()?.GetDataFlow()? == eRender;
            let active = dev.GetState()? == DEVICE_STATE_ACTIVE;
            Ok(Endpoint { id, name, render, active })
        }
    }

    fn matching(en: &IMMDeviceEnumerator, patterns: &[String]) -> windows::core::Result<Vec<(IMMDevice, Endpoint)>> {
        // SAFETY: chiamate COM su interfacce valide.
        unsafe {
            let coll = en.EnumAudioEndpoints(eAll, DEVICE_STATE(DEVICE_STATE_ACTIVE.0 | DEVICE_STATE_UNPLUGGED.0))?;
            let mut out = Vec::new();
            for i in 0..coll.GetCount()? {
                let dev = coll.Item(i)?;
                if let Ok(ep) = describe(&dev) {
                    if name_matches(&ep.name, patterns) {
                        out.push((dev, ep));
                    }
                }
            }
            Ok(out)
        }
    }

    pub fn endpoints(patterns: &[String]) -> Result<Vec<Endpoint>, String> {
        let en = enumerator().map_err(|e| e.message())?;
        Ok(matching(&en, patterns).map_err(|e| e.message())?.into_iter().map(|(_, e)| e).collect())
    }

    /// Dall'endpoint risale al filtro KS del driver audio Bluetooth.
    fn ks_filter(en: &IMMDeviceEnumerator, dev: &IMMDevice) -> windows::core::Result<(String, IKsControl)> {
        // SAFETY: catena di chiamate COM documentata (IDeviceTopology → IConnector → IPart).
        unsafe {
            let topo: IDeviceTopology = dev.Activate(CLSCTX_ALL, None)?;
            let conn: IConnector = topo.GetConnector(0)?;
            let other: IConnector = conn.GetConnectedTo()?;
            let part: IPart = other.cast()?;
            let filter_topo: IDeviceTopology = part.GetTopologyObject()?;
            let filter_id = take_pwstr(filter_topo.GetDeviceId()?);
            let wide: Vec<u16> = filter_id.encode_utf16().chain([0]).collect();
            let filter_dev = en.GetDevice(PCWSTR(wide.as_ptr()))?;
            let ks: IKsControl = filter_dev.Activate(CLSCTX_ALL, None)?;
            Ok((filter_id, ks))
        }
    }

    fn oneshot(ks: &IKsControl, id: u32) -> windows::core::Result<()> {
        let prop = KSIDENTIFIER {
            Anonymous: windows::Win32::Media::KernelStreaming::KSIDENTIFIER_0 {
                Anonymous: KSIDENTIFIER_0_0 { Set: KSPROPSETID_BTAUDIO, Id: id, Flags: KSPROPERTY_TYPE_GET },
            },
        };
        let mut returned = 0u32;
        // SAFETY: KSIDENTIFIER valido, nessun buffer dati richiesto da questa proprietà.
        unsafe {
            ks.KsProperty(
                &prop,
                std::mem::size_of::<KSIDENTIFIER>() as u32,
                std::ptr::null_mut(),
                0,
                &mut returned,
            )
        }
    }

    /// Solo lettura: quanti filtri KS distinti si raggiungono dagli endpoint
    /// delle cuffie (usato dallo smoke test, non invia comandi al driver).
    #[cfg(test)]
    pub fn ks_filter_count(patterns: &[String]) -> usize {
        let Ok(en) = enumerator() else { return 0 };
        let mut ids: Vec<String> = matching(&en, patterns)
            .unwrap_or_default()
            .iter()
            .filter_map(|(dev, _)| ks_filter(&en, dev).ok().map(|(id, _)| id))
            .collect();
        ids.sort();
        ids.dedup();
        ids.len()
    }

    pub fn set_connected(patterns: &[String], connect: bool) -> Result<usize, String> {
        let en = enumerator().map_err(|e| e.message())?;
        let devices = matching(&en, patterns).map_err(|e| e.message())?;
        if devices.is_empty() {
            return Err(crate::t!("err.audio.no_endpoint"));
        }
        let prop = if connect { KSPROPERTY_ONESHOT_RECONNECT } else { KSPROPERTY_ONESHOT_DISCONNECT };
        let mut done: Vec<String> = Vec::new();
        let mut last_err = None;
        for (dev, _) in &devices {
            match ks_filter(&en, dev) {
                Ok((fid, ks)) if !done.contains(&fid) => match oneshot(&ks, prop) {
                    Ok(()) => done.push(fid),
                    Err(e) => last_err = Some(e.message()),
                },
                Ok(_) => {}
                Err(e) => last_err = Some(e.message()),
            }
        }
        if done.is_empty() {
            Err(crate::t!(
                "err.audio.rejected",
                detail = last_err.map(|e| format!(": {e}")).unwrap_or_default()
            ))
        } else {
            Ok(done.len())
        }
    }

    /// Imposta come predefinito (Console + Multimedia) l'endpoint di
    /// riproduzione attivo delle cuffie, preferendo quello stereo (A2DP)
    /// rispetto al vivavoce (HFP).
    pub fn set_default(patterns: &[String]) -> Result<String, String> {
        let en = enumerator().map_err(|e| e.message())?;
        let mut eps: Vec<Endpoint> = matching(&en, patterns)
            .map_err(|e| e.message())?
            .into_iter()
            .map(|(_, e)| e)
            .filter(|e| e.render && e.active)
            .collect();
        let is_handsfree = |n: &str| {
            let n = n.to_lowercase();
            ["hands-free", "handsfree", "vivavoce", "headset", "auricolare"].iter().any(|k| n.contains(k))
        };
        eps.sort_by_key(|e| is_handsfree(&e.name));
        let ep = eps.first().ok_or_else(|| crate::t!("err.audio.not_connected"))?;
        set_default_endpoint(&ep.id)?;
        Ok(ep.name.clone())
    }

    /// Imposta come predefinito (Console + Multimedia) l'endpoint con questo id.
    pub fn set_default_by_id(id: &str) -> Result<String, String> {
        let en = enumerator().map_err(|e| e.message())?;
        // SAFETY: id terminato da 0, interfacce COM valide.
        let name = unsafe {
            let wide: Vec<u16> = id.encode_utf16().chain([0]).collect();
            let dev = en.GetDevice(PCWSTR(wide.as_ptr())).map_err(|_| crate::t!("err.audio.device_not_found"))?;
            let ep = describe(&dev).map_err(|e| e.message())?;
            if !(ep.render && ep.active) {
                return Err(crate::t!("err.audio.device_inactive"));
            }
            ep.name
        };
        set_default_endpoint(id)?;
        Ok(name)
    }

    fn set_default_endpoint(id: &str) -> Result<(), String> {
        let wide: Vec<u16> = id.encode_utf16().chain([0]).collect();
        // SAFETY: IPolicyConfig è implementata da CPolicyConfigClient; id terminato da 0.
        unsafe {
            let pc: IPolicyConfig = CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL).map_err(|e| e.message())?;
            for role in [eConsole, eMultimedia] {
                policy::set_default_endpoint(&pc, PCWSTR(wide.as_ptr()), role.0).ok().map_err(|e| e.message())?;
            }
        }
        Ok(())
    }

    /// Tutti gli endpoint di riproduzione attivi (cuffie Bluetooth o cablate,
    /// altoparlanti, HDMI...), con l'id da salvare nelle impostazioni.
    pub fn render_endpoints() -> Result<Vec<Endpoint>, String> {
        let en = enumerator().map_err(|e| e.message())?;
        // SAFETY: chiamate COM su interfacce valide.
        let eps = unsafe {
            let coll = en.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE).map_err(|e| e.message())?;
            let n = coll.GetCount().map_err(|e| e.message())?;
            (0..n).filter_map(|i| coll.Item(i).ok()).filter_map(|d| describe(&d).ok()).collect::<Vec<_>>()
        };
        Ok(eps.into_iter().filter(|e| e.render && e.active).collect())
    }
}

#[cfg(windows)]
pub use imp::{endpoints, render_endpoints, set_connected, set_default, set_default_by_id};
#[cfg(all(windows, test))]
pub use imp::ks_filter_count;

#[cfg(not(windows))]
pub fn endpoints(_: &[String]) -> Result<Vec<Endpoint>, String> {
    Ok(Vec::new())
}
#[cfg(not(windows))]
pub fn set_connected(_: &[String], _: bool) -> Result<usize, String> {
    Err(crate::t!("err.unsupported"))
}
#[cfg(not(windows))]
pub fn set_default(_: &[String]) -> Result<String, String> {
    Err(crate::t!("err.unsupported"))
}
#[cfg(not(windows))]
pub fn set_default_by_id(_: &str) -> Result<String, String> {
    Err(crate::t!("err.unsupported"))
}
#[cfg(not(windows))]
pub fn render_endpoints() -> Result<Vec<Endpoint>, String> {
    Ok(Vec::new())
}
