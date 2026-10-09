use tauri::AppHandle;

pub(crate) fn set_application_icon(app: AppHandle, icon: Vec<u8>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        app.run_on_main_thread(move || {
            let result = set_macos_application_icon(&icon);
            let _ = sender.send(result);
        })
        .map_err(|error| error.to_string())?;

        receiver
            .recv()
            .map_err(|error| format!("failed to update macOS application icon: {error}"))??;
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, icon);
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn set_macos_application_icon(icon: &[u8]) -> Result<(), String> {
    use objc2::{AllocAnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    let marker = unsafe { MainThreadMarker::new_unchecked() };
    let application = NSApplication::sharedApplication(marker);
    let data = NSData::with_bytes(icon);
    let image = NSImage::initWithData(NSImage::alloc(), &data)
        .ok_or_else(|| "failed to decode application icon PNG".to_string())?;

    unsafe {
        application.setApplicationIconImage(Some(&image));
    }
    Ok(())
}
