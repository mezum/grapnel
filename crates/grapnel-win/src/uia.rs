//! UI Automation lookups of the focused element on a worker thread (they can take tens of ms).

use grapnel_config::Matcher;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationCondition, IUIAutomationElement, IUIAutomationValuePattern,
    TreeScope_Subtree, UIA_ControlTypePropertyId, UIA_EditControlTypeId, UIA_IsValuePatternAvailablePropertyId,
    UIA_ValueIsReadOnlyPropertyId, UIA_ValuePatternId,
};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
use windows::core::BSTR;

/// AutomationId, Name and ControlType name of the focused element.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Focused {
    pub id: String,
    pub name: String,
    pub control_type: String,
}

pub struct Uia {
    tx: Sender<u64>,
    /// Number of the last `refresh`; results for older ones are stale.
    asked: AtomicU64,
    /// The latest result and the refresh it answers.
    latest: Arc<Mutex<(u64, Focused)>>,
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

/// The focused input, or the nearest one around the focused element: Chromium reports a list's
/// active row as focused rather than the input that owns the list (VS Code's command palette). With
/// `into`, only an input whose name matches. The search stays inside the focused top-level window.
fn find_input(
    uia: &IUIAutomation,
    editable: &IUIAutomationCondition,
    into: Option<&Matcher>,
) -> windows::core::Result<Option<IUIAutomationElement>> {
    unsafe {
        let (walker, desktop) = (uia.ControlViewWalker()?, uia.GetRootElement()?);
        let mut at = uia.GetFocusedElement()?;
        // ponytail: looks a few levels up only; a deeper owner would need a smarter search.
        for _ in 0..8 {
            if let Ok(all) = at.FindAll(TreeScope_Subtree, editable) {
                for i in 0..all.Length()? {
                    let e = all.GetElement(i)?;
                    if into.is_none_or(|m| e.CurrentName().is_ok_and(|n| m.is_match(&n.to_string()))) {
                        return Ok(Some(e));
                    }
                }
            }
            let Ok(parent) = walker.GetParentElement(&at) else { break };
            if uia.CompareElements(&parent, &desktop)?.as_bool() {
                break;
            }
            at = parent;
        }
        Ok(None)
    }
}

/// Replaces the text of the input `find_input` finds. With `into`, waits up to a second for that
/// input to appear. `Ok(false)` when there is none, or once `live` says the write is no longer
/// wanted (checked before each search and before writing). It blocks for as long as UI Automation
/// takes, so call it off the hook thread.
pub fn set_text(text: &str, into: Option<&Matcher>, live: impl Fn() -> bool) -> windows::core::Result<bool> {
    // Each call runs on a thread of its own, so COM is set up and torn down around it.
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let done = write_input(text, into, live);
    if com.is_ok() {
        unsafe { CoUninitialize() }; // after every UI Automation object above is dropped
    }
    done
}

fn write_input(text: &str, into: Option<&Matcher>, live: impl Fn() -> bool) -> windows::core::Result<bool> {
    // ponytail: fixed wait, make it a step option if some input takes longer to appear.
    let deadline = Instant::now() + Duration::from_secs(1);
    unsafe {
        let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        // Only edits: Chromium gives VS Code's palette list the input's name and a ValuePattern that
        // accepts a value without taking it.
        let editable = uia.CreateAndCondition(
            &uia.CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(UIA_EditControlTypeId.0))?,
            &uia.CreateAndCondition(
                &uia.CreatePropertyCondition(UIA_IsValuePatternAvailablePropertyId, &VARIANT::from(true))?,
                &uia.CreatePropertyCondition(UIA_ValueIsReadOnlyPropertyId, &VARIANT::from(false))?,
            )?,
        )?;
        while live() {
            if let Some(input) = find_input(&uia, &editable, into)? {
                let value: IUIAutomationValuePattern = input.GetCurrentPatternAs(UIA_ValuePatternId)?;
                if !live() {
                    break;
                }
                value.SetValue(&BSTR::from(text))?;
                return Ok(true);
            }
            if into.is_none() || Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(false)
    }
}

impl Uia {
    /// Starts the worker. After each `refresh`, it posts `msg` to `hwnd` once the result is ready.
    pub fn spawn(hwnd: HWND, msg: u32) -> Uia {
        let (tx, rx) = channel::<u64>();
        let latest = Arc::new(Mutex::new((0, Focused::default())));
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
            while let Ok(mut asked) = rx.recv() {
                asked = rx.try_iter().last().unwrap_or(asked); // coalesce bursts of focus events
                let focused = query(&uia).unwrap_or_default();
                *out.lock().unwrap() = (asked, focused);
                let _ = unsafe { PostMessageW(Some(HWND(target as _)), msg, WPARAM(0), LPARAM(0)) };
            }
        });
        Uia { tx, asked: AtomicU64::new(0), latest }
    }

    pub fn refresh(&self) {
        let _ = self.tx.send(self.asked.fetch_add(1, Ordering::Relaxed) + 1);
    }

    /// The focused element as of the last `refresh`; `None` while that result is still coming.
    pub fn latest(&self) -> Option<Focused> {
        let (answers, focused) = self.latest.lock().unwrap().clone();
        (answers == self.asked.load(Ordering::Relaxed)).then_some(focused)
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

    #[test]
    fn results_for_older_refreshes_are_stale() {
        use super::*;
        let latest = Arc::new(Mutex::new((1, Focused { id: "old".into(), ..Default::default() })));
        let u = Uia { tx: channel().0, asked: AtomicU64::new(2), latest: latest.clone() };
        assert_eq!(u.latest(), None);
        latest.lock().unwrap().0 = 2;
        assert_eq!(u.latest().unwrap().id, "old");
    }
}
