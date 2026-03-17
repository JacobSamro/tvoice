#![allow(unexpected_cfgs)]

use crate::ui::UiCommand;
use anyhow::{Context, Result, bail};
use cocoa::appkit::{
    NSButton, NSColor, NSImage, NSImageNameInfo, NSImageNameRevealFreestandingTemplate,
    NSImageNameStopProgressFreestandingTemplate, NSMenu, NSMenuItem, NSRectFill,
    NSSquareStatusItemLength, NSStatusBar, NSStatusItem,
};
use cocoa::base::{NO, YES, id, nil};
use cocoa::foundation::{NSPoint, NSRect, NSSize, NSString};
use objc::declare::ClassDecl;
use objc::rc::StrongPtr;
use objc::runtime::{Class, Object, Sel};
use objc::{class, msg_send, sel, sel_impl};
use std::ffi::c_void;
use std::sync::OnceLock;
use std::sync::mpsc::Sender;

const CALLBACKS_IVAR: &str = "callbacks";

type MenuHandler = Box<dyn FnMut()>;

struct MenuCallbacks {
    open_app: MenuHandler,
    about: MenuHandler,
    quit: MenuHandler,
}

pub struct StatusItemController {
    item: StrongPtr,
    _menu: StrongPtr,
    _target: StrongPtr,
}

impl StatusItemController {
    pub fn install(ui_tx: Sender<UiCommand>) -> Result<Self> {
        unsafe {
            let button_width = NSSquareStatusItemLength;
            let status_bar = NSStatusBar::systemStatusBar(nil);
            let item = StrongPtr::retain(status_bar.statusItemWithLength_(button_width));
            let button = item.button();

            if button == nil {
                bail!("macOS status item did not provide a button");
            }

            item.setLength_(button_width);
            let _: () = msg_send![button, setHidden: NO];
            button.setTitle_(NSString::alloc(nil).init_str(""));
            let image = status_item_image();
            if image != nil {
                button.setImage_(image);
            } else {
                button.setTitle_(NSString::alloc(nil).init_str("TV"));
            }
            let _: () = msg_send![button, setFrameSize: NSSize::new(button_width, 22.0)];
            let _: () = msg_send![button, setToolTip: NSString::alloc(nil).init_str("tvoice")];

            let target: id = msg_send![status_item_target_class(), new];
            if target == nil {
                bail!("failed to create status item target");
            }

            let callbacks = MenuCallbacks {
                open_app: Box::new({
                    let ui_tx = ui_tx.clone();
                    move || {
                        let _ = ui_tx.send(UiCommand::ShowSettings);
                    }
                }),
                about: Box::new({
                    let ui_tx = ui_tx.clone();
                    move || {
                        let _ = ui_tx.send(UiCommand::ShowAbout);
                    }
                }),
                quit: Box::new(quit_application),
            };

            (*target).set_ivar(
                CALLBACKS_IVAR,
                Box::into_raw(Box::new(callbacks)) as *mut c_void,
            );

            let menu = build_status_item_menu(target)?;
            item.setMenu_(*menu);

            Ok(Self {
                item,
                _menu: menu,
                _target: StrongPtr::new(target),
            })
        }
    }
}

impl Drop for StatusItemController {
    fn drop(&mut self) {
        unsafe {
            let status_bar = NSStatusBar::systemStatusBar(nil);
            status_bar.removeStatusItem_(*self.item);
        }
    }
}

fn status_item_target_class() -> &'static Class {
    static CLASS: OnceLock<&'static Class> = OnceLock::new();

    CLASS.get_or_init(|| unsafe {
        let mut decl = ClassDecl::new("TvoiceStatusItemTarget", class!(NSObject))
            .context("failed to declare TvoiceStatusItemTarget")
            .unwrap();
        decl.add_ivar::<*mut c_void>(CALLBACKS_IVAR);
        decl.add_method(
            sel!(openApp:),
            open_app_selected as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(showAbout:),
            show_about_selected as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(quitApp:),
            quit_app_selected as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(sel!(dealloc), dealloc_target as extern "C" fn(&Object, Sel));
        decl.register()
    })
}

fn build_status_item_menu(target: id) -> Result<StrongPtr> {
    unsafe {
        let menu = NSMenu::alloc(nil).initWithTitle_(NSString::alloc(nil).init_str("tvoice"));
        if menu == nil {
            bail!("failed to create the status item menu");
        }

        menu.setAutoenablesItems(NO);
        add_menu_item(
            menu,
            "Open App",
            sel!(openApp:),
            target,
            NSImageNameRevealFreestandingTemplate,
        )?;
        add_menu_item(menu, "About", sel!(showAbout:), target, NSImageNameInfo)?;
        add_menu_item(
            menu,
            "Quit",
            sel!(quitApp:),
            target,
            NSImageNameStopProgressFreestandingTemplate,
        )?;

        Ok(StrongPtr::new(menu))
    }
}

fn add_menu_item(menu: id, title: &str, action: Sel, target: id, image_name: id) -> Result<()> {
    unsafe {
        let item = NSMenuItem::alloc(nil).initWithTitle_action_keyEquivalent_(
            NSString::alloc(nil).init_str(title),
            action,
            NSString::alloc(nil).init_str(""),
        );
        if item == nil {
            bail!("failed to create a status menu item");
        }

        NSMenuItem::setTarget_(item, target);
        let image = NSImage::imageNamed_(nil, image_name);
        if image != nil {
            let _: () = msg_send![image, setTemplate: YES];
            let _: () = msg_send![image, setSize: NSSize::new(14.0, 14.0)];
            let _: () = msg_send![item, setImage: image];
        }
        menu.addItem_(item);
        Ok(())
    }
}

fn quit_application() {
    unsafe {
        let _: () = msg_send![cocoa::appkit::NSApp(), terminate: nil];
    }
}

fn status_item_image() -> id {
    unsafe {
        let size = NSSize::new(18.0, 18.0);
        let image = NSImage::alloc(nil).initWithSize_(size);
        if image == nil {
            return nil;
        }

        image.lockFocus();

        let fill = NSColor::colorWithCalibratedRed_green_blue_alpha_(nil, 0.0, 0.0, 0.0, 1.0);
        let _: () = msg_send![fill, set];

        NSRectFill(NSRect::new(NSPoint::new(3.0, 4.0), NSSize::new(3.0, 7.0)));
        NSRectFill(NSRect::new(NSPoint::new(7.5, 3.0), NSSize::new(3.0, 11.0)));
        NSRectFill(NSRect::new(NSPoint::new(12.0, 5.0), NSSize::new(3.0, 6.0)));

        image.unlockFocus();
        let _: () = msg_send![image, setTemplate: YES];
        let _: () = msg_send![image, setSize: size];

        image
    }
}

extern "C" fn open_app_selected(this: &Object, _: Sel, _: id) {
    with_callbacks(this, |callbacks| (callbacks.open_app)());
}

extern "C" fn show_about_selected(this: &Object, _: Sel, _: id) {
    with_callbacks(this, |callbacks| (callbacks.about)());
}

extern "C" fn quit_app_selected(this: &Object, _: Sel, _: id) {
    with_callbacks(this, |callbacks| (callbacks.quit)());
}

fn with_callbacks(this: &Object, action: impl FnOnce(&mut MenuCallbacks)) {
    unsafe {
        let raw_callbacks = *this.get_ivar::<*mut c_void>(CALLBACKS_IVAR);
        if raw_callbacks.is_null() {
            return;
        }

        let callbacks = &mut *(raw_callbacks as *mut MenuCallbacks);
        action(callbacks);
    }
}

extern "C" fn dealloc_target(this: &Object, _: Sel) {
    unsafe {
        let raw_callbacks = *this.get_ivar::<*mut c_void>(CALLBACKS_IVAR);
        if !raw_callbacks.is_null() {
            drop(Box::from_raw(raw_callbacks as *mut MenuCallbacks));
        }

        let _: () = msg_send![super(this, class!(NSObject)), dealloc];
    }
}
