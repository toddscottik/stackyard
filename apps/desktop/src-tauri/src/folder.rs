use tauri::WebviewWindow;

#[tauri::command]
pub async fn pick_project_parent(window: WebviewWindow) -> Result<Option<String>, String> {
    let (send, receive) = tokio::sync::oneshot::channel();
    let parent = window.clone();
    window
        .run_on_main_thread(move || {
            let _ = send.send(pick(&parent));
        })
        .map_err(|error| error.to_string())?;
    receive.await.map_err(|error| error.to_string())?
}

#[cfg(windows)]
fn pick(window: &WebviewWindow) -> Result<Option<String>, String> {
    use windows::core::{w, HRESULT};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{
        FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR, FOS_PATHMUSTEXIST,
        FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
    };
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            // Balance initialization on this same thread.
            unsafe { CoUninitialize() };
        }
    }
    let owner = window.hwnd().map_err(|error| error.to_string())?;
    let result = (|| -> windows::core::Result<Option<String>> {
        // The callback runs on the UI thread, which owns the window and dialog.
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            let _apartment = Apartment;
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            dialog.SetOptions(
                dialog.GetOptions()?
                    | FOS_PICKFOLDERS
                    | FOS_FORCEFILESYSTEM
                    | FOS_PATHMUSTEXIST
                    | FOS_NOCHANGEDIR,
            )?;
            dialog.SetTitle(w!("Choose a parent folder for the project"))?;
            match dialog.Show(Some(owner)) {
                Err(error) if error.code() == HRESULT::from_win32(1223) => return Ok(None),
                result => result?,
            }
            let path = dialog.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
            let value = path.to_string();
            CoTaskMemFree(Some(path.0.cast()));
            value.map(Some).map_err(Into::into)
        }
    })();
    result.map_err(|error| format!("Folder picker: {error}"))
}

#[cfg(target_os = "macos")]
fn pick(_window: &WebviewWindow) -> Result<Option<String>, String> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};

    let main = MainThreadMarker::new().ok_or("Folder picker requires the main thread")?;
    // AppKit owns the panel and returns a retained file URL for the selection.
    unsafe {
        let panel = NSOpenPanel::openPanel(main);
        panel.setCanChooseFiles(false);
        panel.setCanChooseDirectories(true);
        panel.setAllowsMultipleSelection(false);
        if panel.runModal() != NSModalResponseOK {
            return Ok(None);
        }
        let path = panel
            .URL()
            .and_then(|url| url.path())
            .ok_or("Folder picker did not return a filesystem path")?;
        Ok(Some(path.to_string()))
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn pick(_window: &WebviewWindow) -> Result<Option<String>, String> {
    Err("The native folder picker is unavailable on this target. Enter an absolute path.".into())
}
