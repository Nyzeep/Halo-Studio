//! macOS Native Menubar

#[cfg(target_os = "macos")]
use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenubarMode {
    Startup,
    Workspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditMenuMode {
    System,
    Renderer,
}

pub const MENU_ID_EDIT_UNDO: &str = "halo.edit.undo";
pub const MENU_ID_EDIT_REDO: &str = "halo.edit.redo";
pub const MENU_ID_EDIT_CUT: &str = "halo.edit.cut";
pub const MENU_ID_EDIT_COPY: &str = "halo.edit.copy";
pub const MENU_ID_EDIT_PASTE: &str = "halo.edit.paste";
pub const MENU_ID_EDIT_SELECT_ALL: &str = "halo.edit.select_all";

pub fn menu_event_name_for_id(id: &str) -> Option<&'static str> {
    match id {
        "halo.open_project" => Some("halo_menu_open_project"),
        "halo.new_project" => Some("halo_menu_new_project"),
        "halo.about" => Some("halo_menu_about"),
        MENU_ID_EDIT_UNDO => Some("halo_menu_edit_undo"),
        MENU_ID_EDIT_REDO => Some("halo_menu_edit_redo"),
        MENU_ID_EDIT_CUT => Some("halo_menu_edit_cut"),
        MENU_ID_EDIT_COPY => Some("halo_menu_edit_copy"),
        MENU_ID_EDIT_PASTE => Some("halo_menu_edit_paste"),
        MENU_ID_EDIT_SELECT_ALL => Some("halo_menu_edit_select_all"),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone)]
struct MenubarLabels {
    project_menu: &'static str,
    edit_menu: &'static str,
    open_project: &'static str,
    new_project: &'static str,
    about_halo: &'static str,
    undo: &'static str,
    redo: &'static str,
    cut: &'static str,
    copy: &'static str,
    paste: &'static str,
    select_all: &'static str,
}

#[cfg(target_os = "macos")]
fn labels_for_language(language: &str) -> MenubarLabels {
    match language {
        "en-US" => MenubarLabels {
            project_menu: "Project",
            edit_menu: "Edit",
            open_project: "Open Project…",
            new_project: "New Project…",
            about_halo: "About Halo Studio",
            undo: "Undo",
            redo: "Redo",
            cut: "Cut",
            copy: "Copy",
            paste: "Paste",
            select_all: "Select All",
        },
        "zh-TW" => MenubarLabels {
            project_menu: "工程",
            edit_menu: "編輯",
            open_project: "開啟工程…",
            new_project: "新建工程…",
            about_halo: "關於 Halo Studio",
            undo: "復原",
            redo: "重做",
            cut: "剪下",
            copy: "複製",
            paste: "貼上",
            select_all: "全選",
        },
        _ => MenubarLabels {
            project_menu: "工程",
            edit_menu: "编辑",
            open_project: "打开工程…",
            new_project: "新建工程…",
            about_halo: "关于 Halo Studio",
            undo: "撤销",
            redo: "重做",
            cut: "剪切",
            copy: "复制",
            paste: "粘贴",
            select_all: "全选",
        },
    }
}

#[cfg(target_os = "macos")]
pub fn set_macos_menubar_with_mode(
    app: &tauri::AppHandle,
    language: &str,
    mode: MenubarMode,
    edit_mode: EditMenuMode,
) -> tauri::Result<()> {
    let labels = labels_for_language(language);
    let _ = mode;

    let app_menu = SubmenuBuilder::new(app, "Halo")
        .text("halo.about", labels.about_halo)
        .separator()
        .quit()
        .build()?;

    let edit_menu = match edit_mode {
        EditMenuMode::System => SubmenuBuilder::new(app, labels.edit_menu)
            .undo()
            .redo()
            .separator()
            .cut()
            .copy()
            .paste()
            .select_all()
            .build()?,
        EditMenuMode::Renderer => {
            let undo = MenuItemBuilder::with_id(MENU_ID_EDIT_UNDO, labels.undo)
                .accelerator("Cmd+Z")
                .build(app)?;
            let redo = MenuItemBuilder::with_id(MENU_ID_EDIT_REDO, labels.redo)
                .accelerator("Cmd+Shift+Z")
                .build(app)?;
            let cut = MenuItemBuilder::with_id(MENU_ID_EDIT_CUT, labels.cut)
                .accelerator("Cmd+X")
                .build(app)?;
            let copy = MenuItemBuilder::with_id(MENU_ID_EDIT_COPY, labels.copy)
                .accelerator("Cmd+C")
                .build(app)?;
            let paste = MenuItemBuilder::with_id(MENU_ID_EDIT_PASTE, labels.paste)
                .accelerator("Cmd+V")
                .build(app)?;
            let select_all = MenuItemBuilder::with_id(MENU_ID_EDIT_SELECT_ALL, labels.select_all)
                .accelerator("Cmd+A")
                .build(app)?;

            SubmenuBuilder::new(app, labels.edit_menu)
                .item(&undo)
                .item(&redo)
                .separator()
                .item(&cut)
                .item(&copy)
                .item(&paste)
                .item(&select_all)
                .build()?
        }
    };

    let project_menu = SubmenuBuilder::new(app, labels.project_menu)
        .text("halo.open_project", labels.open_project)
        .text("halo.new_project", labels.new_project)
        .build()?;

    let menu = MenuBuilder::new(app)
        .item(&app_menu)
        .item(&edit_menu)
        .item(&project_menu)
        .build()?;

    app.set_menu(menu)?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn set_macos_menubar_with_mode(
    _app: &tauri::AppHandle,
    _language: &str,
    _mode: MenubarMode,
    _edit_mode: EditMenuMode,
) -> tauri::Result<()> {
    Ok(())
}
