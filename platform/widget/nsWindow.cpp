/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "nsWindow.h"

#include <linux/input-event-codes.h>

#include "BandeKeys.h"
#include "GtkCompositorWidget.h"
#include "ScreenHelperBande.h"
#include "WaylandVsyncSource.h"
#include "mozilla/BasicEvents.h"
#include "mozilla/MouseEvents.h"
#include "mozilla/StaticPrefs_widget.h"
#include "mozilla/TextEventDispatcher.h"
#include "mozilla/TextEventDispatcherListener.h"
#include "mozilla/TextEvents.h"
#include "mozilla/WidgetUtils.h"
#include "mozilla/XREAppData.h"
#include "mozilla/dom/MouseEventBinding.h"
#include "mozilla/dom/WheelEventBinding.h"
#include "mozilla/layers/WebRenderBridgeChild.h"
#include "mozilla/layers/WebRenderLayerManager.h"
#include "mozilla/widget/WaylandSurface.h"
#include "nsAppRunner.h"
#include "nsTHashMap.h"
#include "nsXULAppAPI.h"
#include "PreferenceSheet.h"

using namespace mozilla;
using namespace mozilla::dom;
using namespace mozilla::layers;
using namespace mozilla::widget;

static const uint32_t kBandeButtonPrimary = 0;
static const uint32_t kBandeButtonMiddle = 1;
static const uint32_t kBandeButtonSecondary = 2;
static const uint32_t kBandeButtonBack = 3;
static const uint32_t kBandeButtonForward = 4;
static const uint32_t kBandeScrollWheel = 0;
static const double kWheelLinesPerPoint = 3.0 / 120.0;

static nsTHashMap<uint64_t, nsWindow*>& LiveWindows() {
  static nsTHashMap<uint64_t, nsWindow*> sWindows;
  return sWindows;
}

class BandeTextListener final : public TextEventDispatcherListener {
 public:
  NS_DECL_ISUPPORTS

  NS_IMETHOD NotifyIME(TextEventDispatcher*, const IMENotification&) override {
    return NS_OK;
  }
  NS_IMETHOD_(IMENotificationRequests) GetIMENotificationRequests() override {
    return IMENotificationRequests();
  }
  NS_IMETHOD_(void) OnRemovedFrom(TextEventDispatcher*) override {}
  NS_IMETHOD_(void)
  WillDispatchKeyboardEvent(TextEventDispatcher*, WidgetKeyboardEvent&,
                            uint32_t, void*) override {}

 private:
  ~BandeTextListener() override = default;
};

NS_IMPL_ISUPPORTS(BandeTextListener, TextEventDispatcherListener,
                  nsISupportsWeakReference)

already_AddRefed<nsIWidget> nsIWidget::CreateTopLevelWindow() {
  nsCOMPtr<nsIWidget> window = new nsWindow();
  return window.forget();
}

already_AddRefed<nsIWidget> nsIWidget::CreateChildWindow() {
  nsCOMPtr<nsIWidget> window = new nsWindow();
  return window.forget();
}

nsWindow::nsWindow() = default;

nsWindow::~nsWindow() { Destroy(); }

nsresult nsWindow::Create(nsIWidget* aParent, const LayoutDeviceIntRect& aRect,
                          const InitData& aInitData) {
  BaseCreate(aParent, aInitData);
  mBounds = aRect;
  ScreenHelperBande::EnsureStarted();
  mContainer.mSurface = new WaylandSurface();
  mContainer.mSurface->Init();
  mContainer.mSurface->SetLoggingWidget(this);
  if (IsTopLevelWidget() &&
      StaticPrefs::widget_wayland_vsync_enabled_AtStartup()) {
    mVsyncSource = new WaylandVsyncSource(this);
    mVsyncSource->Init();
    mVsyncDispatcher = new VsyncDispatcher(mVsyncSource);
    mVsyncSource->EnableVSyncSource();
  }
  return NS_OK;
}

void nsWindow::Destroy() {
  if (mIsDestroyed) {
    return;
  }
  mIsDestroyed = true;
  mCreated = false;
  Show(false);
  DestroyCompositor();
  if (mVsyncSource) {
    mVsyncSource->Shutdown();
    mVsyncSource = nullptr;
  }
  mVsyncDispatcher = nullptr;
  if (mContainer.mSurface) {
    WaylandSurfaceLock lock(mContainer.mSurface);
    if (mContainer.mSurface->IsMapped()) {
      mContainer.mSurface->UnmapLocked(lock);
    }
    mContainer.mSurface->GdkCleanUpLocked(lock);
  }
  mContainer.mSurface = nullptr;
  OnDestroy();
}

double nsWindow::FractionalScaleFactor() const {
  if (mContainer.mSurface) {
    double scale = mContainer.mSurface->GetScale();
    if (scale != sNoScale) {
      return scale;
    }
  }
  return mBandeWindow ? BandeHost::Scale(mBandeWindow) : 1.0;
}

double nsWindow::GetDefaultScaleInternal() { return FractionalScaleFactor(); }

DesktopToLayoutDeviceScale nsWindow::GetDesktopToDeviceScale() const {
  return DesktopToLayoutDeviceScale(FractionalScaleFactor());
}

LayoutDeviceIntPoint nsWindow::ToDevice(double aX, double aY) const {
  double scale = FractionalScaleFactor();
  return LayoutDeviceIntPoint(int32_t(aX * scale), int32_t(aY * scale));
}

void nsWindow::MapToplevel() {
  double scale = FractionalScaleFactor();
  nsAutoCString appId(gAppData ? gAppData->remotingName : "org.bande.Browser");
  mBandeWindow = BandeHost::Open(mTitle.get(), appId.get(),
                                 uint32_t(mBounds.width / scale),
                                 uint32_t(mBounds.height / scale));
  if (!mBandeWindow) {
    return;
  }
  LiveWindows().InsertOrUpdate(mBandeWindow, this);
  WaylandSurface* surface = mContainer.mSurface;
  WaylandSurfaceLock lock(surface);
  if (!surface->MapLocked(lock, BandeHost::Surface(mBandeWindow),
                          DesktopIntPoint())) {
    return;
  }
  surface->SetViewportFollowsSizeChangesLocked(lock);
  surface->SetScaleCallbackLocked(lock,
                                  WaylandSurface::ScaleCallbackType::Widget,
                                  [win = RefPtr{this}]() { win->RefreshScale(); });
  surface->SetScaleTypeLocked(lock, WaylandSurface::ScaleType::Fractional,
                              /* aSetHandler */ true);
  surface->SetOpaqueRegionLocked(lock, mOpaqueRegion.ToUnknownRegion());
  surface->DisableUserInputLocked(lock);
  surface->CommitLocked(lock);
  surface->RunMapCallbackLocked(lock);
  uint32_t width = 0, height = 0;
  BandeHost::Size(mBandeWindow, &width, &height);
  surface->SetSize(DesktopIntSize(int32_t(width), int32_t(height)));
}

void nsWindow::UnmapToplevel() {
  if (!mBandeWindow) {
    return;
  }
  if (mContainer.mSurface) {
    WaylandSurfaceLock lock(mContainer.mSurface);
    if (mContainer.mSurface->IsMapped()) {
      mContainer.mSurface->UnmapLocked(lock);
    }
  }
  LiveWindows().Remove(mBandeWindow);
  BandeHost::Close(mBandeWindow);
  mBandeWindow = 0;
}

void nsWindow::Show(bool aState) {
  if (aState == mVisible) {
    return;
  }
  mVisible = aState;
  if (!BandeHost::IsActive() || mWindowType != WindowType::TopLevel) {
    return;
  }
  if (aState) {
    MapToplevel();
    Paint();
  } else {
    UnmapToplevel();
  }
}

void nsWindow::Paint() {
  if (mIsDestroyed) {
    return;
  }
  nsIWidgetListener* listener = GetPaintListener();
  if (!listener) {
    return;
  }
  listener->WillPaintWindow(this);
  listener = GetPaintListener();
  if (!listener) {
    return;
  }
  WindowRenderer* renderer = GetWindowRenderer();
  if (renderer) {
    if (WebRenderLayerManager* layerManager = renderer->AsWebRender()) {
      if (!mConfiguredClearColor) {
        layerManager->WrBridge()->SendSetDefaultClearColor(LookAndFeel::Color(
            LookAndFeel::ColorID::Window,
            PreferenceSheet::ColorSchemeForChrome(),
            LookAndFeel::UseStandins::No));
        mConfiguredClearColor = true;
      }
      layerManager->SetNeedsComposite(true);
      layerManager->ScheduleComposite(wr::RenderReasons::WIDGET);
      layerManager->SetNeedsComposite(false);
    }
  }
  listener->PaintWindow(this);
  listener = GetPaintListener();
  if (listener) {
    listener->DidPaintWindow();
  }
}

void nsWindow::Invalidate(const LayoutDeviceIntRect&) {
  NS_DispatchToCurrentThread(NS_NewRunnableFunction(
      "nsWindow::Invalidate", [self = RefPtr{this}]() { self->Paint(); }));
}

void nsWindow::Move(const DesktopPoint& aPoint) {
  auto scale = GetDesktopToDeviceScale();
  mBounds.MoveTo(LayoutDeviceIntPoint::Round(aPoint * scale));
}

void nsWindow::Resize(const DesktopSize& aSize, bool aRepaint) {
  auto scale = GetDesktopToDeviceScale();
  LayoutDeviceIntSize size = LayoutDeviceIntSize::Round(aSize * scale);
  if (size == mBounds.Size()) {
    return;
  }
  if (mBandeWindow) {
    BandeHost::SetSize(mBandeWindow, uint32_t(aSize.width),
                       uint32_t(aSize.height));
  }
  RefreshBounds(uint32_t(aSize.width), uint32_t(aSize.height));
}

void nsWindow::Resize(const DesktopRect& aRect, bool aRepaint) {
  Move(aRect.TopLeft());
  Resize(aRect.Size(), aRepaint);
}

void nsWindow::RefreshBounds(uint32_t aWidth, uint32_t aHeight) {
  double scale = FractionalScaleFactor();
  mBounds.SizeTo(int32_t(aWidth * scale + 0.5), int32_t(aHeight * scale + 0.5));
  if (mContainer.mSurface) {
    mContainer.mSurface->SetSize(DesktopIntSize(int32_t(aWidth), int32_t(aHeight)));
  }
  if (mCompositorWidgetDelegate) {
    mCompositorWidgetDelegate->NotifyClientSizeChanged(mBounds.Size());
  }
  if (mWidgetListener) {
    mWidgetListener->WindowResized(this, mBounds.Size());
  }
  if (mAttachedWidgetListener) {
    mAttachedWidgetListener->WindowResized(this, mBounds.Size());
  }
}

void nsWindow::RefreshScale() {
  if (!mBandeWindow) {
    return;
  }
  NotifyAPZOfDPIChange();
  uint32_t width = 0, height = 0;
  BandeHost::Size(mBandeWindow, &width, &height);
  RefreshBounds(width, height);
  if (mWidgetListener) {
    mWidgetListener->UIResolutionChanged();
  }
}

void nsWindow::SetSizeMode(nsSizeMode aMode) {
  if (!mBandeWindow || aMode == mSizeMode) {
    return;
  }
  switch (aMode) {
    case nsSizeMode_Minimized:
      BandeHost::Minimize(mBandeWindow);
      break;
    case nsSizeMode_Maximized:
    case nsSizeMode_Normal:
      if (mSizeMode == nsSizeMode_Fullscreen) {
        BandeHost::SetFullScreen(mBandeWindow, false);
      } else if ((aMode == nsSizeMode_Maximized) !=
                 (mSizeMode == nsSizeMode_Maximized)) {
        BandeHost::ToggleZoom(mBandeWindow);
      }
      break;
    default:
      break;
  }
}

nsresult nsWindow::MakeFullScreen(bool aFullScreen) {
  if (mBandeWindow) {
    BandeHost::SetFullScreen(mBandeWindow, aFullScreen);
  }
  return NS_OK;
}

void* nsWindow::GetNativeData(uint32_t aDataType) {
  switch (aDataType) {
    case NS_NATIVE_WINDOW:
    case NS_NATIVE_WIDGET:
      return this;
    case NS_NATIVE_EGL_WINDOW: {
      double scale = FractionalScaleFactor();
      return mContainer.mSurface
                 ? mContainer.mSurface->GetEGLWindow(
                       DesktopIntSize(int32_t(mBounds.width / scale),
                                      int32_t(mBounds.height / scale)))
                 : nullptr;
    }
    default:
      return nullptr;
  }
}

nsresult nsWindow::SetTitle(const nsAString& aTitle) {
  CopyUTF16toUTF8(aTitle, mTitle);
  if (mBandeWindow) {
    BandeHost::SetTitle(mBandeWindow, mTitle.get());
  }
  return NS_OK;
}

static uint32_t BandeCursorOf(nsCursor aCursor) {
  switch (aCursor) {
    case eCursor_hyperlink: return 1;
    case eCursor_select:
    case eCursor_vertical_text: return 2;
    case eCursor_crosshair: return 3;
    case eCursor_move: return 4;
    case eCursor_grab: return 5;
    case eCursor_grabbing: return 6;
    case eCursor_not_allowed:
    case eCursor_no_drop: return 7;
    case eCursor_context_menu: return 8;
    case eCursor_copy: return 9;
    case eCursor_alias: return 10;
    case eCursor_col_resize:
    case eCursor_ew_resize: return 11;
    case eCursor_row_resize:
    case eCursor_ns_resize: return 12;
    case eCursor_n_resize: return 13;
    case eCursor_s_resize: return 14;
    case eCursor_w_resize: return 15;
    case eCursor_e_resize: return 16;
    case eCursor_nw_resize:
    case eCursor_nwse_resize: return 17;
    case eCursor_ne_resize:
    case eCursor_nesw_resize: return 18;
    case eCursor_sw_resize: return 19;
    case eCursor_se_resize: return 20;
    default: return 0;
  }
}

void nsWindow::SetCursor(const Cursor& aCursor) {
  if (mBandeWindow) {
    BandeHost::SetCursor(mBandeWindow, BandeCursorOf(aCursor.mDefaultCursor));
  }
}

UniquePtr<WaylandSurfaceLock> nsWindow::LockSurface() {
  if (mIsDestroyed || !mContainer.mSurface) {
    return nullptr;
  }
  return MakeUnique<WaylandSurfaceLock>(mContainer.mSurface);
}

void nsWindow::UpdateOpaqueRegion(const LayoutDeviceIntRegion& aRegion) {
  mOpaqueRegion = aRegion;
  if (mContainer.mSurface) {
    mContainer.mSurface->SetOpaqueRegion(mOpaqueRegion.ToUnknownRegion());
  }
}

void nsWindow::GetCompositorWidgetInitData(
    CompositorWidgetInitData* aInitData) {
  *aInitData = GtkCompositorWidgetInitData(0, nsCString(), false, GetClientSize());
}

void nsWindow::SetCompositorWidgetDelegate(CompositorWidgetDelegate* aDelegate) {
  mCompositorWidgetDelegate =
      aDelegate ? aDelegate->AsPlatformSpecificDelegate() : nullptr;
}

void nsWindow::CreateCompositorVsyncDispatcher() {
  if (!mVsyncSource) {
    nsIWidget::CreateCompositorVsyncDispatcher();
    return;
  }
  if (!mCompositorVsyncDispatcherLock) {
    mCompositorVsyncDispatcherLock =
        MakeUnique<Mutex>("mCompositorVsyncDispatcherLock");
  }
  MutexAutoLock lock(*mCompositorVsyncDispatcherLock);
  if (!mCompositorVsyncDispatcher) {
    mCompositorVsyncDispatcher = new CompositorVsyncDispatcher(mVsyncDispatcher);
  }
}

RefPtr<VsyncDispatcher> nsWindow::GetVsyncDispatcher() {
  return mVsyncDispatcher;
}

bool nsWindow::WidgetTypeSupportsAcceleration() {
  return mWindowType != WindowType::Popup || HasRemoteContent();
}

bool nsWindow::WidgetTypeSupportsNativeCompositing() { return true; }

TextEventDispatcherListener* nsWindow::GetNativeTextEventDispatcherListener() {
  static StaticRefPtr<BandeTextListener> sListener;
  if (!sListener) {
    sListener = new BandeTextListener();
    ClearOnShutdown(&sListener);
  }
  return sListener;
}

static void AppendCommand(nsTArray<CommandInt>& aCommands, Command aCommand) {
  aCommands.AppendElement(static_cast<CommandInt>(aCommand));
}

bool nsWindow::GetEditCommands(NativeKeyBindingsType aType,
                               const WidgetKeyboardEvent& aEvent,
                               nsTArray<CommandInt>& aCommands) {
  if (NS_WARN_IF(!nsIWidget::GetEditCommands(aType, aEvent, aCommands))) {
    return false;
  }
  if (aType == NativeKeyBindingsType::RichTextEditor ||
      aType == NativeKeyBindingsType::MultiLineEditor ||
      aType == NativeKeyBindingsType::SingleLineEditor) {
    const bool shift = aEvent.IsShift();
    const bool word = aEvent.IsControl();
    switch (aEvent.mKeyNameIndex) {
      case KEY_NAME_INDEX_Backspace:
        AppendCommand(aCommands, word ? Command::DeleteWordBackward
                                      : Command::DeleteCharBackward);
        break;
      case KEY_NAME_INDEX_Delete:
        AppendCommand(aCommands, word ? Command::DeleteWordForward
                                      : Command::DeleteCharForward);
        break;
      case KEY_NAME_INDEX_ArrowLeft:
        AppendCommand(aCommands,
                      word ? (shift ? Command::SelectWordPrevious
                                    : Command::WordPrevious)
                           : (shift ? Command::SelectCharPrevious
                                    : Command::CharPrevious));
        break;
      case KEY_NAME_INDEX_ArrowRight:
        AppendCommand(aCommands, word ? (shift ? Command::SelectWordNext
                                               : Command::WordNext)
                                      : (shift ? Command::SelectCharNext
                                               : Command::CharNext));
        break;
      case KEY_NAME_INDEX_Home:
        AppendCommand(aCommands, shift ? Command::SelectBeginLine
                                       : Command::BeginLine);
        break;
      case KEY_NAME_INDEX_End:
        AppendCommand(aCommands,
                      shift ? Command::SelectEndLine : Command::EndLine);
        break;
      case KEY_NAME_INDEX_ArrowUp:
        if (aType != NativeKeyBindingsType::SingleLineEditor) {
          AppendCommand(aCommands, shift ? Command::SelectLinePrevious
                                         : Command::LinePrevious);
        }
        break;
      case KEY_NAME_INDEX_ArrowDown:
        if (aType != NativeKeyBindingsType::SingleLineEditor) {
          AppendCommand(aCommands,
                        shift ? Command::SelectLineNext : Command::LineNext);
        }
        break;
      default:
        break;
    }
  }
  return true;
}

Modifiers nsWindow::ModifiersOf(uint32_t aBits) const {
  Modifiers modifiers = MODIFIER_NONE;
  if (aBits & BandeModifierShift) {
    modifiers |= MODIFIER_SHIFT;
  }
  if (aBits & BandeModifierControl) {
    modifiers |= MODIFIER_CONTROL;
  }
  if (aBits & BandeModifierAlt) {
    modifiers |= MODIFIER_ALT;
  }
  if (aBits & BandeModifierLogo) {
    modifiers |= MODIFIER_META;
  }
  return modifiers;
}

static int16_t DomButton(uint32_t aButton) {
  switch (aButton) {
    case kBandeButtonMiddle: return MouseButton::eMiddle;
    case kBandeButtonSecondary: return MouseButton::eSecondary;
    case kBandeButtonBack: return MouseButton::eX1;
    case kBandeButtonForward: return MouseButton::eX2;
    case kBandeButtonPrimary:
    default: return MouseButton::ePrimary;
  }
}

static uint16_t DomButtonBit(uint32_t aButton) {
  switch (aButton) {
    case kBandeButtonMiddle: return MouseButtonsFlag::eMiddleFlag;
    case kBandeButtonSecondary: return MouseButtonsFlag::eSecondaryFlag;
    case kBandeButtonBack: return MouseButtonsFlag::e4thFlag;
    case kBandeButtonForward: return MouseButtonsFlag::e5thFlag;
    case kBandeButtonPrimary:
    default: return MouseButtonsFlag::ePrimaryFlag;
  }
}

void nsWindow::DispatchPointer(const BandeEvent& aEvent) {
  EventMessage message = eMouseMove;
  switch (aEvent.kind) {
    case BandeEventKind::PointerEnter:
      message = eMouseEnterIntoWidget;
      break;
    case BandeEventKind::PointerLeave:
      message = eMouseExitFromWidget;
      break;
    case BandeEventKind::PointerDown:
      message = eMouseDown;
      mPressedButtons |= DomButtonBit(aEvent.a);
      break;
    case BandeEventKind::PointerUp:
      message = eMouseUp;
      mPressedButtons &= ~DomButtonBit(aEvent.a);
      break;
    default:
      break;
  }
  WidgetMouseEvent event(true, message, this, WidgetMouseEvent::eReal);
  event.mRefPoint = ToDevice(aEvent.x, aEvent.y);
  event.mModifiers = ModifiersOf(aEvent.modifiers);
  event.mButtons = mPressedButtons;
  event.mInputSource = MouseEvent_Binding::MOZ_SOURCE_MOUSE;
  if (message == eMouseExitFromWidget) {
    event.mExitFrom = Some(WidgetMouseEvent::ePlatformTopLevel);
  }
  if (message == eMouseDown || message == eMouseUp) {
    event.mButton = DomButton(aEvent.a);
    event.mClickCount = aEvent.b;
  }
  DispatchInputEvent(&event);
  if (message == eMouseDown && event.mButton == MouseButton::eSecondary &&
      !mIsDestroyed) {
    WidgetPointerEvent contextMenu(true, eContextMenu, this,
                                   WidgetMouseEvent::eReal);
    contextMenu.mRefPoint = ToDevice(aEvent.x, aEvent.y);
    contextMenu.mModifiers = ModifiersOf(aEvent.modifiers);
    contextMenu.mButton = MouseButton::eSecondary;
    contextMenu.mButtons = mPressedButtons;
    DispatchInputEvent(&contextMenu);
  }
}

void nsWindow::DispatchScroll(const BandeEvent& aEvent) {
  WidgetWheelEvent event(true, eWheel, this);
  event.mRefPoint = ToDevice(aEvent.x, aEvent.y);
  event.mModifiers = ModifiersOf(aEvent.modifiers);
  if (aEvent.a == kBandeScrollWheel) {
    event.mDeltaMode = WheelEvent_Binding::DOM_DELTA_LINE;
    event.mDeltaX = aEvent.dx * kWheelLinesPerPoint;
    event.mDeltaY = aEvent.dy * kWheelLinesPerPoint;
    event.mLineOrPageDeltaX = int32_t(event.mDeltaX);
    event.mLineOrPageDeltaY = int32_t(event.mDeltaY);
  } else {
    double scale = FractionalScaleFactor();
    event.mDeltaMode = WheelEvent_Binding::DOM_DELTA_PIXEL;
    event.mDeltaX = aEvent.dx * scale;
    event.mDeltaY = aEvent.dy * scale;
    event.mIsNoLineOrPageDelta = true;
    event.mScrollType = WidgetWheelEvent::SCROLL_ASYNCHRONOUSLY;
  }
  DispatchInputEvent(&event);
}

void nsWindow::DispatchKey(const BandeEvent& aEvent) {
  RefPtr<TextEventDispatcher> dispatcher = GetTextEventDispatcher();
  if (NS_FAILED(dispatcher->BeginNativeInputTransaction())) {
    return;
  }
  const bool pressed = aEvent.b & 1;
  const bool repeat = aEvent.b & 2;
  WidgetKeyboardEvent event(true, pressed ? eKeyDown : eKeyUp, this);
  event.mModifiers = ModifiersOf(aEvent.modifiers);
  event.mCodeNameIndex = CodeNameFromEvdev(aEvent.a);
  event.mKeyNameIndex = KeyNameFromKeysym(aEvent.c);
  event.mKeyCode = LegacyKeyCode(aEvent.c, aEvent.d);
  event.mIsRepeat = repeat;
  if (event.mKeyNameIndex == KEY_NAME_INDEX_USE_STRING) {
    uint32_t character = CharacterFromKeysym(aEvent.c);
    if (character) {
      AppendUCS4ToUTF16(character, event.mKeyValue);
    } else {
      event.mKeyNameIndex = KEY_NAME_INDEX_Unidentified;
    }
  }
  nsEventStatus status = nsEventStatus_eIgnore;
  if (pressed) {
    if (dispatcher->DispatchKeyboardEvent(eKeyDown, event, status, nullptr)) {
      dispatcher->MaybeDispatchKeypressEvents(event, status, nullptr);
    }
  } else {
    dispatcher->DispatchKeyboardEvent(eKeyUp, event, status, nullptr);
  }
}

void nsWindow::DispatchFocus(bool aFocused) {
  if (!mWidgetListener) {
    return;
  }
  if (aFocused) {
    mWidgetListener->WindowActivated();
  } else {
    mWidgetListener->WindowDeactivated();
  }
}

void nsWindow::DispatchToplevelState(uint32_t aState) {
  nsSizeMode mode = nsSizeMode_Normal;
  if (aState & BandeStateFullscreen) {
    mode = nsSizeMode_Fullscreen;
  } else if (aState & BandeStateMaximized) {
    mode = nsSizeMode_Maximized;
  }
  if (mode != mSizeMode) {
    mSizeMode = mode;
    if (mCompositorWidgetDelegate) {
      mCompositorWidgetDelegate->NotifyFullscreenChanged(
          mode == nsSizeMode_Fullscreen);
    }
    if (mWidgetListener) {
      mWidgetListener->SizeModeChanged(mSizeMode);
    }
  }
}

void nsWindow::OnBandeEvent(const BandeEvent& aEvent) {
  switch (aEvent.kind) {
    case BandeEventKind::PointerEnter:
    case BandeEventKind::PointerLeave:
    case BandeEventKind::PointerMove:
    case BandeEventKind::PointerDown:
    case BandeEventKind::PointerUp:
      DispatchPointer(aEvent);
      break;
    case BandeEventKind::Scroll:
      DispatchScroll(aEvent);
      break;
    case BandeEventKind::Key:
      DispatchKey(aEvent);
      break;
    case BandeEventKind::Focus:
      DispatchFocus(aEvent.a);
      break;
    case BandeEventKind::Close:
      if (mWidgetListener) {
        mWidgetListener->RequestWindowClose(this);
      }
      break;
    case BandeEventKind::ToplevelState:
      DispatchToplevelState(aEvent.a);
      break;
    case BandeEventKind::Resized:
      RefreshBounds(aEvent.a, aEvent.b);
      break;
    default:
      break;
  }
}

void nsWindow::HandleBandeEvent(const BandeEvent& aEvent) {
  nsWindow* window = LiveWindows().Get(aEvent.window);
  if (window) {
    RefPtr<nsWindow> kungFuDeathGrip = window;
    kungFuDeathGrip->OnBandeEvent(aEvent);
  }
}

#include "mozilla/widget/ScreenManager.h"

void ScreenHelperBande::EnsureStarted() {
  static bool sStarted = false;
  if (sStarted || !XRE_IsParentProcess()) {
    return;
  }
  sStarted = true;
  const LayoutDeviceIntRect rect(0, 0, 1920, 1200);
  AutoTArray<RefPtr<Screen>, 1> screens;
  screens.AppendElement(MakeRefPtr<Screen>(
      rect, rect, 24, 24, 0, DesktopToLayoutDeviceScale(),
      CSSToLayoutDeviceScale(), 96.0f, Screen::IsPseudoDisplay::No,
      Screen::IsHDR::No, 80.0f, 1000.0f));
  ScreenManager::Refresh(std::move(screens));
}
