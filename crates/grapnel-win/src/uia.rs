//! UI Automation lookups of the focused element on a worker thread (they can take tens of ms).

use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

/// AutomationId, Name and ControlType name of the focused element.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Focused {
    pub id: String,
    pub name: String,
    pub control_type: String,
}

pub struct Uia {
    tx: Sender<()>,
    latest: Arc<Mutex<Focused>>,
}

const CONTROL_TYPES: [&str; 39] = [
    "Button",
    "Calendar",
    "CheckBox",
    "ComboBox",
    "Edit",
    "Hyperlink",
    "Image",
    "ListItem",
    "List",
    "Menu",
    "MenuBar",
    "MenuItem",
    "ProgressBar",
    "RadioButton",
    "ScrollBar",
    "Slider",
    "Spinner",
    "StatusBar",
    "Tab",
    "TabItem",
    "Text",
    "ToolBar",
    "ToolTip",
    "Tree",
    "TreeItem",
    "Custom",
    "Group",
    "Thumb",
    "DataGrid",
    "DataItem",
    "Document",
    "SplitButton",
    "Window",
    "Pane",
    "Header",
    "HeaderItem",
    "Table",
    "TitleBar",
    "Separator",
];

fn control_type_name(id: i32) -> String {
    let name = usize::try_from(id - 50000).ok().and_then(|i| CONTROL_TYPES.get(i));
    name.map(|s| s.to_string()).unwrap_or_else(|| id.to_string())
}

fn query(uia: &IUIAutomation) -> windows::core::Result<Focused> {
    unsafe {
        let el = uia.GetFocusedElement()?;
        Ok(Focused {
            id: el.CurrentAutomationId()?.to_string(),
            name: el.CurrentName()?.to_string(),
            control_type: control_type_name(el.CurrentControlType()?.0),
        })
    }
}

impl Uia {
    /// Starts the worker. After each `refresh`, it posts `msg` to `hwnd` once the result is ready.
    pub fn spawn(hwnd: HWND, msg: u32) -> Uia {
        let (tx, rx) = channel::<()>();
        let latest = Arc::new(Mutex::new(Focused::default()));
        let out = latest.clone();
        let target = hwnd.0 as isize;
        std::thread::spawn(move || {
            let uia: IUIAutomation = unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                match CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
                    Ok(u) => u,
                    Err(e) => return log::error!("UI Automation unavailable: {e}"),
                }
            };
            while rx.recv().is_ok() {
                while rx.try_recv().is_ok() {} // coalesce bursts of focus events
                let focused = query(&uia).unwrap_or_default();
                *out.lock().unwrap() = focused;
                let _ = unsafe { PostMessageW(Some(HWND(target as _)), msg, WPARAM(0), LPARAM(0)) };
            }
        });
        Uia { tx, latest }
    }

    pub fn refresh(&self) {
        let _ = self.tx.send(());
    }

    pub fn latest(&self) -> Focused {
        self.latest.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn control_type_names() {
        assert_eq!(super::control_type_name(50004), "Edit");
        assert_eq!(super::control_type_name(50038), "Separator");
        assert_eq!(super::control_type_name(50100), "50100");
    }
}
