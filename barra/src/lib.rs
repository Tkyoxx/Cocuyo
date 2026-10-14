// esto vive dentro del explorador de windows 11. si algo aqui revienta se cae la barra entera, asi que cero unwrap

#![allow(non_snake_case)]

use std::collections::HashSet;
use std::ffi::c_void;
use std::sync::Mutex;

use windows::core::{implement, interface, w, IUnknown, IUnknown_Vtbl, Interface, Ref, BOOL, BSTR, GUID, HRESULT};
use windows::Win32::Foundation::{CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_FAIL, E_POINTER, HANDLE, RECT, S_FALSE, S_OK};
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, IClassFactory, IClassFactory_Impl, COINIT_MULTITHREADED};
use windows::Win32::System::Ole::{IObjectWithSite, IObjectWithSite_Impl};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForMultipleObjects, INFINITE};

pub const CLSID: GUID = GUID::from_u128(0x6f3c1a52_8e0d_4c1b_9a37_c0c0a1b2c3d4);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SourceInfo {
    file_name: *const u16,
    line: u32,
    column: u32,
    position: u32,
    hash: *const u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ParentChildRelation {
    parent: u64,
    child: u64,
    child_index: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VisualElement {
    handle: u64,
    source: SourceInfo,
    kind: *const u16,
    name: *const u16,
    children: u32,
}

#[repr(C)]
pub struct PropertyChainSource {
    handle: u64,
    target_type: *const u16,
    name: *const u16,
    source: i32,
    info: SourceInfo,
}

#[repr(C)]
pub struct PropertyChainValue {
    index: u32,
    kind: *const u16,
    declaring_type: *const u16,
    value_type: *const u16,
    item_type: *const u16,
    value: *const u16,
    overridden: BOOL,
    metadata: i64,
    property_name: *const u16,
    chain_index: u32,
}

#[interface("aa7a8931-80e4-4fec-8f3b-553f87b4966e")]
unsafe trait IVisualTreeServiceCallback: IUnknown {
    fn OnVisualTreeChange(&self, relation: ParentChildRelation, element: VisualElement, mutation: i32) -> HRESULT;
}

#[interface("a593b11a-d17f-48bb-8f66-83910731c8a5")]
unsafe trait IVisualTreeService: IUnknown {
    fn AdviseVisualTreeChange(&self, callback: *mut c_void) -> HRESULT;
    fn UnadviseVisualTreeChange(&self, callback: *mut c_void) -> HRESULT;
    fn GetEnums(&self, count: *mut u32, enums: *mut *mut c_void) -> HRESULT;
    fn CreateInstance(&self, type_name: *const u16, value: *const u16, handle: *mut u64) -> HRESULT;
    fn GetPropertyValuesChain(&self, instance: u64, source_count: *mut u32, sources: *mut *mut PropertyChainSource, value_count: *mut u32, values: *mut *mut PropertyChainValue) -> HRESULT;
    fn SetProperty(&self, instance: u64, value: u64, index: u32) -> HRESULT;
    fn ClearProperty(&self, instance: u64, index: u32) -> HRESULT;
}

#[interface("18c9e2b6-3f43-4116-9f2b-ff935d7770d2")]
unsafe trait IXamlDiagnostics: IUnknown {
    fn GetDispatcher(&self, out: *mut *mut c_void) -> HRESULT;
    fn GetUiLayer(&self, out: *mut *mut c_void) -> HRESULT;
    fn GetApplication(&self, out: *mut *mut c_void) -> HRESULT;
    fn GetIInspectableFromHandle(&self, handle: u64, out: *mut *mut c_void) -> HRESULT;
    fn GetHandleFromIInspectable(&self, inspectable: *mut c_void, out: *mut u64) -> HRESULT;
    fn HitTest(&self, rect: RECT, count: *mut u32, out: *mut *mut u64) -> HRESULT;
    fn RegisterInstance(&self, inspectable: *mut c_void, out: *mut u64) -> HRESULT;
    fn GetInitializationData(&self, out: *mut *const u16) -> HRESULT;
}

struct Shared<T>(T);
unsafe impl<T> Send for Shared<T> {}

#[derive(Default)]
struct Tree {
    inside: HashSet<u64>,
    targets: Vec<u64>,
    probe: Option<String>,
}

static TREE: Mutex<Option<Tree>> = Mutex::new(None);
static LINK: Mutex<Option<Shared<(IVisualTreeService, IVisualTreeServiceCallback)>>> = Mutex::new(None);
static NUDGE: Mutex<Option<Shared<HANDLE>>> = Mutex::new(None);
static OPACITY: Mutex<Option<(u32, u64)>> = Mutex::new(None);

fn text(raw: *const u16) -> String {
    if raw.is_null() {
        return String::new();
    }
    let mut len = 0;
    unsafe {
        while len < 512 && *raw.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(raw, len))
    }
}

fn nudge() {
    if let Ok(nudge) = NUDGE.lock() {
        if let Some(event) = nudge.as_ref() {
            unsafe {
                let _ = SetEvent(event.0);
            }
        }
    }
}

fn wanted_hidden(probe: bool) -> bool {
    if probe {
        return true;
    }
    let mut value = 0u32;
    let mut size = 4u32;
    let read = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, w!("Software\\Cocuyo"), w!("BarraSinFondo"), RRF_RT_REG_DWORD, None, Some((&mut value as *mut u32).cast()), Some(&mut size))
    };
    read.is_ok() && value == 1
}

unsafe fn free(raw: *const u16) {
    if !raw.is_null() {
        drop(BSTR::from_raw(raw));
    }
}

unsafe fn opacity(service: &IVisualTreeService, target: u64) -> Option<(u32, u64)> {
    if let Ok(known) = OPACITY.lock() {
        if known.is_some() {
            return *known;
        }
    }
    let (mut source_count, mut sources, mut value_count, mut values) = (0u32, std::ptr::null_mut(), 0u32, std::ptr::null_mut());
    if service.GetPropertyValuesChain(target, &mut source_count, &mut sources, &mut value_count, &mut values).is_err() {
        return None;
    }
    let mut index = None;
    if !values.is_null() {
        for i in 0..value_count as usize {
            let value: &PropertyChainValue = &*values.add(i);
            if index.is_none() && text(value.property_name) == "Opacity" {
                index = Some(value.index);
            }
            for field in [value.kind, value.declaring_type, value.value_type, value.item_type, value.value, value.property_name] {
                free(field);
            }
        }
        CoTaskMemFree(Some(values as *const c_void));
    }
    if !sources.is_null() {
        for i in 0..source_count as usize {
            let source: &PropertyChainSource = &*sources.add(i);
            for field in [source.target_type, source.name, source.info.file_name, source.info.hash] {
                free(field);
            }
        }
        CoTaskMemFree(Some(sources as *const c_void));
    }
    let index = index?;
    let (kind, zero) = (BSTR::from("Windows.Foundation.Double"), BSTR::from("0"));
    let mut handle = 0u64;
    if service.CreateInstance(kind.as_ptr(), zero.as_ptr(), &mut handle).is_err() || handle == 0 {
        return None;
    }
    if let Ok(mut known) = OPACITY.lock() {
        *known = Some((index, handle));
    }
    Some((index, handle))
}

fn apply() {
    let Some((service, targets, probe)) = (|| {
        let link = LINK.lock().ok()?;
        let service = link.as_ref()?.0 .0.clone();
        let tree = TREE.lock().ok()?;
        let tree = tree.as_ref()?;
        Some((service, tree.targets.clone(), tree.probe.is_some()))
    })() else {
        return;
    };
    let hide = wanted_hidden(probe);
    for target in targets {
        unsafe {
            let Some((index, zero)) = opacity(&service, target) else { return };
            let _ = if hide { service.SetProperty(target, zero, index) } else { service.ClearProperty(target, index) };
        }
    }
}

fn start_worker() {
    let Ok(mut nudge) = NUDGE.lock() else { return };
    if nudge.is_some() {
        return;
    }
    let Ok(inner) = (unsafe { CreateEventW(None, false, false, None) }) else { return };
    *nudge = Some(Shared(inner));
    drop(nudge);
    let outer = unsafe { CreateEventW(None, false, false, w!("Local\\CocuyoBarra")) }.ok();
    let events = Shared((inner, outer));
    let _ = std::thread::Builder::new().name("cocuyo-barra".into()).spawn(move || {
        let events = events;
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let mut handles = vec![events.0 .0];
        if let Some(outer) = events.0 .1 {
            handles.push(outer);
        }
        loop {
            let woke = unsafe { WaitForMultipleObjects(&handles, false, INFINITE) };
            if woke.0 as usize >= handles.len() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
            apply();
        }
    });
}

#[implement(IVisualTreeServiceCallback)]
struct Watcher;

impl IVisualTreeServiceCallback_Impl for Watcher_Impl {
    unsafe fn OnVisualTreeChange(&self, relation: ParentChildRelation, element: VisualElement, mutation: i32) -> HRESULT {
        let Ok(mut tree) = TREE.lock() else { return S_OK };
        let Some(tree) = tree.as_mut() else { return S_OK };
        if mutation != 0 {
            tree.inside.remove(&element.handle);
            tree.targets.retain(|target| *target != element.handle);
            return S_OK;
        }
        let kind = text(element.kind);
        let mut found = false;
        if let Some(probe) = &tree.probe {
            found = kind == *probe && tree.targets.len() < 2000;
        } else if kind == "Taskbar.TaskbarBackground" {
            tree.inside.insert(element.handle);
        } else if tree.inside.contains(&relation.parent) {
            tree.inside.insert(element.handle);
            let name = text(element.name);
            found = name == "BackgroundFill" || name == "BackgroundStroke";
        }
        if found && !tree.targets.contains(&element.handle) {
            tree.targets.push(element.handle);
            nudge();
        }
        S_OK
    }
}

#[implement(IObjectWithSite)]
struct Tap {
    site: Mutex<Option<IUnknown>>,
}

impl IObjectWithSite_Impl for Tap_Impl {
    fn SetSite(&self, site: Ref<IUnknown>) -> windows::core::Result<()> {
        let Ok(site) = site.ok() else { return Ok(()) };
        if let Ok(mut mine) = self.site.lock() {
            *mine = Some(site.clone());
        }
        let service: IVisualTreeService = site.cast()?;
        let mut probe = None;
        if let Ok(diagnostics) = site.cast::<IXamlDiagnostics>() {
            let mut raw: *const u16 = std::ptr::null();
            if unsafe { diagnostics.GetInitializationData(&mut raw) }.is_ok() && !raw.is_null() {
                let data = text(raw);
                unsafe { free(raw) };
                probe = data.strip_prefix("prueba:").map(str::to_owned);
            }
        }
        if let Ok(mut tree) = TREE.lock() {
            *tree = Some(Tree { probe, ..Tree::default() });
        }
        if let Ok(mut known) = OPACITY.lock() {
            *known = None;
        }
        start_worker();
        let work = Shared(service);
        let _ = std::thread::Builder::new().name("cocuyo-barra-alta".into()).spawn(move || {
            let work = work;
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                let previous = LINK.lock().ok().and_then(|mut link| link.take());
                if let Some(Shared((old_service, old_callback))) = previous {
                    let _ = old_service.UnadviseVisualTreeChange(old_callback.as_raw());
                }
                let callback: IVisualTreeServiceCallback = Watcher.into();
                if let Ok(mut link) = LINK.lock() {
                    *link = Some(Shared((work.0.clone(), callback.clone())));
                }
                let _ = work.0.AdviseVisualTreeChange(callback.as_raw());
            }
            nudge();
        });
        Ok(())
    }

    fn GetSite(&self, riid: *const GUID, out: *mut *mut c_void) -> windows::core::Result<()> {
        if out.is_null() || riid.is_null() {
            return Err(E_POINTER.into());
        }
        let site = self.site.lock().ok().and_then(|site| site.clone());
        match site {
            Some(site) => unsafe { site.query(riid, out).ok() },
            None => Err(E_FAIL.into()),
        }
    }
}

#[implement(IClassFactory)]
struct Factory;

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(&self, outer: Ref<IUnknown>, riid: *const GUID, out: *mut *mut c_void) -> windows::core::Result<()> {
        if out.is_null() || riid.is_null() {
            return Err(E_POINTER.into());
        }
        if outer.is_some() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let tap: IObjectWithSite = Tap { site: Mutex::new(None) }.into();
        unsafe { tap.query(riid, out).ok() }
    }

    fn LockServer(&self, _lock: BOOL) -> windows::core::Result<()> {
        Ok(())
    }
}

#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(clsid: *const GUID, riid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    if clsid.is_null() || riid.is_null() || out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if *clsid != CLSID {
        return CLASS_E_CLASSNOTAVAILABLE;
    }
    let factory: IClassFactory = Factory.into();
    factory.query(riid, out)
}

#[no_mangle]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    S_FALSE
}

