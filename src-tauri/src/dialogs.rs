// los dialogos de elegir carpeta y archivo

use std::path::PathBuf;

use windows::core::{w, HSTRING};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FileOpenDialog, FileSaveDialog, IFileOpenDialog, IFileSaveDialog, IShellItem,
    FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};

pub(crate) fn on_sta<T: Send + 'static>(f: impl FnOnce() -> Option<T> + Send + 'static) -> Option<T> {
    std::thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let result = f();
        CoUninitialize();
        result
    })
    .join()
    .ok()
    .flatten()
}

unsafe fn path_of(item: IShellItem) -> Option<PathBuf> {
    let raw = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
    let path = raw.to_string().ok().map(PathBuf::from);
    CoTaskMemFree(Some(raw.0 as _));
    path
}

pub fn pick_folder() -> Option<PathBuf> {
    on_sta(|| unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let options = dialog.GetOptions().ok()?;
        dialog.SetOptions(options | FOS_PICKFOLDERS).ok()?;
        dialog.Show(None).ok()?;
        path_of(dialog.GetResult().ok()?)
    })
}

pub fn pick_program() -> Option<PathBuf> {
    on_sta(|| unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let name = windows::core::HSTRING::from(crate::i18n::tr("Programas y accesos directos"));
        let types = [COMDLG_FILTERSPEC { pszName: windows::core::PCWSTR(name.as_ptr()), pszSpec: w!("*.exe;*.lnk;*.url;*.bat") }];
        dialog.SetFileTypes(&types).ok()?;
        dialog.Show(None).ok()?;
        path_of(dialog.GetResult().ok()?)
    })
}

pub fn save_png(suggested: String) -> Option<PathBuf> {
    on_sta(move || unsafe {
        let dialog: IFileSaveDialog =
            CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let types = [COMDLG_FILTERSPEC { pszName: w!("Imagen PNG"), pszSpec: w!("*.png") }];
        dialog.SetFileTypes(&types).ok()?;
        dialog.SetDefaultExtension(w!("png")).ok()?;
        dialog.SetFileName(&HSTRING::from(suggested)).ok()?;
        dialog.Show(None).ok()?;
        path_of(dialog.GetResult().ok()?)
    })
}
