/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "nsWindow.h"

#include "BandeKeys.h"
#include "NativeKeyBindings.h"
#include "nsDragService.h"
#include "nsIRollupListener.h"
#include "GtkCompositorWidget.h"
#include "ScreenHelperBande.h"
#include "WaylandVsyncSource.h"
#include "mozilla/BasicEvents.h"
#include "mozilla/ClearOnShutdown.h"
#include "mozilla/LookAndFeel.h"
#include "mozilla/PresShell.h"
#include "mozilla/StaticPtr.h"
#include "mozilla/widget/PlatformWidgetTypes.h"
#include "mozilla/MouseEvents.h"
#include "mozilla/StaticPrefs_widget.h"
#include "mozilla/TextEventDispatcher.h"
#include "mozilla/TextEventDispatcherListener.h"
#include "mozilla/TextEvents.h"
#include "mozilla/ContentEvents.h"
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
#include "mozilla/PreferenceSheet.h"

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

bool gUseStableRounding = true;

static nsTHashMap<uint64_t, nsWindow*>& LiveWindows() {
  static nsTHashMap<uint64_t, nsWindow*> sWindows;
  return sWindows;
}

class BandeTextListener final : public TextEventDispatcherListener {
 public:
  NS_DECL_ISUPPORTS

  NS_IMETHOD NotifyIME(TextEventDispatcher* aDispatcher,
                       const IMENotification& aNotification) override {
    RefPtr<nsWindow> window = nsWindow::FromWidget(aDispatcher->GetWidget());
    if (!window) {
      return NS_OK;
    }
    nsEventStatus status = nsEventStatus_eIgnore;
    switch (aNotification.mMessage) {
      case NOTIFY_IME_OF_FOCUS:
        window->OnIMEFocus(true);
        break;
      case NOTIFY_IME_OF_BLUR:
        window->OnIMEFocus(false);
        break;
      case NOTIFY_IME_OF_SELECTION_CHANGE:
      case NOTIFY_IME_OF_POSITION_CHANGE:
        window->UpdateTextInput();
        break;
      case REQUEST_TO_COMMIT_COMPOSITION:
        if (aDispatcher->IsComposing() &&
            NS_SUCCEEDED(aDispatcher->BeginNativeInputTransaction())) {
          aDispatcher->CommitComposition(status);
        }
        break;
      case REQUEST_TO_CANCEL_COMPOSITION:
        if (aDispatcher->IsComposing() &&
            NS_SUCCEEDED(aDispatcher->BeginNativeInputTransaction())) {
          aDispatcher->CommitComposition(status, &EmptyString());
        }
        break;
      default:
        break;
    }
    return NS_OK;
  }
  NS_IMETHOD_(IMENotificationRequests) GetIMENotificationRequests() override {
    return IMENotificationRequests(IMENotificationRequest::PositionChange);
  }
  NS_IMETHOD_(void) OnRemovedFrom(TextEventDispatcher*) override {}
  NS_IMETHOD_(void)
  WillDispatchKeyboardEvent(TextEventDispatcher*, WidgetKeyboardEvent&,
                            uint32_t, void*) override {}

 private:
  ~BandeTextListener() = default;
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
  mGrabs = mWindowType == WindowType::Popup &&
           aInitData.mPopupHint == PopupType::Menu;
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
    if (scale != WaylandSurface::sNoScale) {
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
  nsAutoCString appId(mAppId);
  if (appId.IsEmpty()) {
    appId.Assign(gAppData ? gAppData->remotingName : "org.bande.Browser");
  }
  mBandeWindow = BandeHost::Open(mTitle.get(), appId.get(),
                                 uint32_t(mBounds.width / scale),
                                 uint32_t(mBounds.height / scale));
  if (!mBandeWindow) {
    return;
  }
  LiveWindows().InsertOrUpdate(mBandeWindow, this);
  WaylandSurface* surface = mContainer.mSurface;
  {
    WaylandSurfaceLock lock(surface);
    if (!MapSurfaceLocked(lock)) {
      return;
    }
  }
  uint32_t width = 0, height = 0;
  BandeHost::Size(mBandeWindow, &width, &height);
  surface->SetSize(DesktopIntSize(int32_t(width), int32_t(height)));
}

bool nsWindow::MapSurfaceLocked(WaylandSurfaceLock& lock) {
  WaylandSurface* surface = mContainer.mSurface;
  if (!surface->MapLocked(lock, BandeHost::Surface(mBandeWindow),
                          DesktopIntPoint())) {
    return false;
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
  return true;
}

nsWindow* nsWindow::ParentWindow() const {
  return static_cast<nsWindow*>(mParent);
}

LayoutDeviceIntPoint nsWindow::ParentOrigin() const {
  nsWindow* parent = ParentWindow();
  return parent ? parent->WidgetToScreenOffset() : LayoutDeviceIntPoint();
}

LayoutDeviceIntPoint nsWindow::WidgetToScreenOffset() {
  return IsPopup() ? mBounds.TopLeft() : LayoutDeviceIntPoint();
}

void nsWindow::MapPopup() {
  nsWindow* parent = ParentWindow();
  while (parent && !parent->mBandeWindow) {
    parent = parent->ParentWindow();
  }
  if (!parent) {
    return;
  }
  double scale = FractionalScaleFactor();
  LayoutDeviceIntPoint position = mBounds.TopLeft() - parent->WidgetToScreenOffset();
  mBandeWindow = BandeHost::OpenPopup(
      parent->mBandeWindow, position.x / scale, position.y / scale,
      mBounds.width / scale, mBounds.height / scale, mGrabs);
  if (!mBandeWindow) {
    return;
  }
  LiveWindows().InsertOrUpdate(mBandeWindow, this);
  WaylandSurface* surface = mContainer.mSurface;
  {
    WaylandSurfaceLock lock(surface);
    surface->SetParentLocked(lock, parent->mContainer.mSurface);
    if (!MapSurfaceLocked(lock)) {
      return;
    }
  }
  surface->SetSize(DesktopIntSize(int32_t(mBounds.width / scale),
                                  int32_t(mBounds.height / scale)));
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
  if (!BandeHost::IsActive() ||
      (mWindowType != WindowType::TopLevel && mWindowType != WindowType::Popup)) {
    return;
  }
  if (aState) {
    if (IsPopup()) {
      MapPopup();
    } else {
      MapToplevel();
    }
    Paint(true);
  } else {
    UnmapToplevel();
  }
}

void nsWindow::Paint(bool aExposed) {
  mPaintPending = false;
  if (mIsDestroyed || !mVisible) {
    return;
  }
  nsIWidgetListener* listener = GetPaintListener();
  if (!listener) {
    return;
  }
  WindowRenderer* renderer = GetWindowRenderer();
  if (renderer) {
    if (WebRenderLayerManager* layerManager = renderer->AsWebRender()) {
      if (!mConfiguredClearColor && !IsPopup()) {
        layerManager->WrBridge()->SendSetDefaultClearColor(LookAndFeel::Color(
            LookAndFeel::ColorID::Window,
            PreferenceSheet::ColorSchemeForChrome(),
            LookAndFeel::UseStandins::No));
        mConfiguredClearColor = true;
      }
      if (aExposed || layerManager->NeedsComposite()) {
        layerManager->ScheduleComposite(wr::RenderReasons::WIDGET);
        layerManager->SetNeedsComposite(false);
      }
    }
  }
  listener->PaintWindow(this);
}

void nsWindow::Invalidate(const LayoutDeviceIntRect&) {
  if (mPaintPending) {
    return;
  }
  mPaintPending = true;
  NS_DispatchToCurrentThread(NS_NewRunnableFunction(
      "nsWindow::Invalidate", [self = RefPtr{this}]() { self->Paint(false); }));
}

void nsWindow::Move(const DesktopPoint& aPoint) {
  auto scale = GetDesktopToDeviceScale();
  mBounds.MoveTo(LayoutDeviceIntPoint::Round(aPoint * scale));
  if (IsPopup() && mBandeWindow) {
    LayoutDeviceIntPoint position = mBounds.TopLeft() - ParentOrigin();
    BandeHost::MovePopup(mBandeWindow, position.x / scale.scale,
                         position.y / scale.scale, mBounds.width / scale.scale,
                         mBounds.height / scale.scale);
  }
}

void nsWindow::Resize(const DesktopSize& aSize, bool aRepaint) {
  auto scale = GetDesktopToDeviceScale();
  LayoutDeviceIntSize size = LayoutDeviceIntSize::Round(aSize * scale);
  if (size == mBounds.Size()) {
    return;
  }
  if (mBandeWindow && IsPopup()) {
    LayoutDeviceIntPoint position = mBounds.TopLeft() - ParentOrigin();
    BandeHost::MovePopup(mBandeWindow, position.x / scale.scale,
                         position.y / scale.scale, aSize.width, aSize.height);
  } else if (mBandeWindow) {
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
  ScreenHelperBande::SetScale(FractionalScaleFactor());
  NotifyAPZOfDPIChange();
  uint32_t width = 0, height = 0;
  BandeHost::Size(mBandeWindow, &width, &height);
  RefreshBounds(width, height);
  if (mWidgetListener) {
    if (PresShell* presShell = mWidgetListener->GetPresShell()) {
      presShell->BackingScaleFactorChanged();
    }
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
    case NS_RAW_NATIVE_IME_CONTEXT: {
      nsIWidget* top = GetTopLevelWidget();
      return top ? top : this;
    }
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

void nsWindow::SetWindowClass(const nsAString&, const nsAString& aClass,
                              const nsAString&) {
  if (!aClass.IsEmpty()) {
    CopyUTF16toUTF8(aClass, mAppId);
  }
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

bool nsWindow::GetEditCommands(NativeKeyBindingsType aType,
                               const WidgetKeyboardEvent& aEvent,
                               nsTArray<CommandInt>& aCommands) {
  if (NS_WARN_IF(!nsIWidget::GetEditCommands(aType, aEvent, aCommands))) {
    return false;
  }
  NativeKeyBindings::GetEditCommandsForTests(aType, aEvent, Nothing(),
                                             aCommands);
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

bool nsWindow::RolledUpOnPress() {
  if (IsPopup()) {
    return false;
  }
  nsIRollupListener* listener = GetActiveRollupListener();
  nsCOMPtr<nsIWidget> rollupWidget = listener ? listener->GetRollupWidget() : nullptr;
  if (!rollupWidget || rollupWidget == this) {
    return false;
  }
  return listener->Rollup({});
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
      RolledUpOnPress();
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
    WidgetPointerEvent contextMenu(true, eContextMenu, this);
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

static LazyLogModule sDragLog("BandeDrag");

void nsWindow::DispatchDrag(const BandeEvent& aEvent) {
  MOZ_LOG(sDragLog, LogLevel::Debug,
          ("drag event %u at %f,%f a=%u len=%zu", uint32_t(aEvent.kind),
           aEvent.x, aEvent.y, aEvent.a, aEvent.len));
  RefPtr<nsDragService> service = nsDragService::GetInstance();
  RefPtr<nsDragSession> session =
      static_cast<nsDragSession*>(service->GetCurrentSession(this));
  const LayoutDeviceIntPoint point = ToDevice(aEvent.x, aEvent.y);
  auto dispatch = [&](EventMessage aMessage) {
    WidgetDragEvent event(true, aMessage, this);
    event.mRefPoint = point;
    DispatchInputEvent(&event);
  };
  auto reply = [&]() {
    if (!session) {
      return;
    }
    bool canDrop = false;
    session->GetCanDrop(&canDrop);
    uint32_t action = nsIDragService::DRAGDROP_ACTION_NONE;
    session->GetDragAction(&action);
    MOZ_LOG(sDragLog, LogLevel::Debug, ("reply canDrop %d action %u", canDrop, action));
    BandeHost::AcceptDrag(mBandeWindow,
                          !canDrop ? 0
                          : (action & nsIDragService::DRAGDROP_ACTION_MOVE) ? 2
                                                                           : 1);
  };
  nsDependentCSubstring uris(reinterpret_cast<const char*>(aEvent.data),
                             aEvent.len);
  switch (aEvent.kind) {
    case BandeEventKind::DragEnter: {
      if (!session) {
        session = static_cast<nsDragSession*>(service->StartDragSession(this));
        session->SetIncoming(""_ns, uris, aEvent.a);
      }
      session->SetDragAction(nsIDragService::DRAGDROP_ACTION_COPY);
      session->SetDragEndPoint(point.x, point.y);
      dispatch(eDragEnter);
      dispatch(eDragOver);
      reply();
      break;
    }
    case BandeEventKind::DragMove:
      if (session) {
        session->SetDragAction(nsIDragService::DRAGDROP_ACTION_COPY);
        session->SetDragEndPoint(point.x, point.y);
        session->FireDragEventAtSource(eDrag, 0);
        dispatch(eDragOver);
        reply();
      }
      break;
    case BandeEventKind::DragLeave:
      if (session) {
        dispatch(eDragExit);
        nsCOMPtr<nsINode> source;
        session->GetSourceNode(getter_AddRefs(source));
        if (!source) {
          session->EndDragSession(false, 0);
        }
      }
      break;
    case BandeEventKind::DragDrop:
      if (session) {
        nsCOMPtr<nsINode> source;
        session->GetSourceNode(getter_AddRefs(source));
        if (!source) {
          session->SetIncoming(aEvent.text ? nsDependentCString(aEvent.text)
                                           : nsDependentCString(""),
                               uris, aEvent.text != nullptr);
        }
        session->SetDragAction(aEvent.a == 2
                                   ? nsIDragService::DRAGDROP_ACTION_MOVE
                                   : nsIDragService::DRAGDROP_ACTION_COPY);
        session->SetDragEndPoint(point.x, point.y);
        dispatch(eDragOver);
        dispatch(eDrop);
        session->EndDragSession(true, 0);
      }
      break;
    case BandeEventKind::DragFinished:
      if (session) {
        session->SetDragAction(aEvent.a == 2
                                   ? nsIDragService::DRAGDROP_ACTION_MOVE
                               : aEvent.a == 1
                                   ? nsIDragService::DRAGDROP_ACTION_COPY
                                   : nsIDragService::DRAGDROP_ACTION_NONE);
        session->EndDragSession(aEvent.a != 0, 0);
      }
      break;
    default:
      break;
  }
}

void nsWindow::UpdateTextInput() {
  if (!mBandeWindow) {
    return;
  }
  const bool enabled = mIMEFocused && mInputContext.mIMEState.mEnabled ==
                                          IMEEnabled::Enabled;
  if (!enabled) {
    BandeHost::SetTextInput(mBandeWindow, false, 0, 0, 0, 0);
    return;
  }
  WidgetQueryContentEvent selection(true, eQuerySelectedText, this);
  DispatchEvent(&selection);
  LayoutDeviceIntRect caret(0, 0, 1, 1);
  if (selection.Succeeded()) {
    WidgetQueryContentEvent caretEvent(true, eQueryCaretRect, this);
    caretEvent.InitForQueryCaretRect(selection.mReply->StartOffset());
    DispatchEvent(&caretEvent);
    if (caretEvent.Succeeded()) {
      caret = caretEvent.mReply->mRect;
    }
  }
  double scale = FractionalScaleFactor();
  BandeHost::SetTextInput(mBandeWindow, true, caret.x / scale, caret.y / scale,
                          std::max(caret.width, 1) / scale,
                          std::max(caret.height, 1) / scale);
}

static uint32_t Utf16Offset(const nsACString& aUtf8, uint32_t aBytes) {
  return NS_ConvertUTF8toUTF16(
             Substring(aUtf8, 0, std::min<uint32_t>(aBytes, aUtf8.Length())))
      .Length();
}

static LazyLogModule sImeLog("BandeIME");

void nsWindow::DispatchComposition(const BandeEvent& aEvent) {
  MOZ_LOG(sImeLog, LogLevel::Debug,
          ("%s '%s' composing %d dispatcher %d",
           aEvent.kind == BandeEventKind::ImeCommit ? "commit" : "preedit",
           aEvent.text ? aEvent.text : "", mComposing,
           GetTextEventDispatcher()->IsComposing()));
  RefPtr<TextEventDispatcher> dispatcher = GetTextEventDispatcher();
  if (NS_FAILED(dispatcher->BeginNativeInputTransaction())) {
    return;
  }
  nsDependentCString utf8(aEvent.text ? aEvent.text : "");
  NS_ConvertUTF8toUTF16 text(utf8);
  nsEventStatus status = nsEventStatus_eIgnore;
  if (aEvent.kind == BandeEventKind::ImeCommit) {
    mComposing = false;
    nsresult rv = dispatcher->CommitComposition(status, &text);
    MOZ_LOG(sImeLog, LogLevel::Debug,
            ("commit rv %x status %d", uint32_t(rv), int(status)));
    return;
  }
  if (text.IsEmpty()) {
    if (mComposing) {
      mComposing = false;
      dispatcher->CommitComposition(status, &EmptyString());
    }
    return;
  }
  mComposing = true;
  if (NS_FAILED(dispatcher->SetPendingCompositionString(text)) ||
      NS_FAILED(dispatcher->AppendClauseToPendingComposition(
          text.Length(), TextRangeType::eRawClause))) {
    return;
  }
  if (aEvent.a) {
    dispatcher->SetCaretInPendingComposition(Utf16Offset(utf8, aEvent.a - 1),
                                             0);
  }
  dispatcher->FlushPendingComposition(status);
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
  const bool activated = aState & BandeStateActivated;
  if (activated != mActivated) {
    mActivated = activated;
    DispatchFocus(activated);
  }
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
    case BandeEventKind::ImePreedit:
    case BandeEventKind::ImeCommit:
      DispatchComposition(aEvent);
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
    case BandeEventKind::PopupMoved: {
      double scale = FractionalScaleFactor();
      LayoutDeviceIntPoint position =
          ParentOrigin() + LayoutDeviceIntPoint(int32_t(aEvent.x * scale),
                                                int32_t(aEvent.y * scale));
      if (position != mBounds.TopLeft()) {
        mBounds.MoveTo(position);
        if (mWidgetListener) {
          mWidgetListener->WindowMoved(this, position, ByMoveToRect::No);
        }
      }
      break;
    }
    case BandeEventKind::DragEnter:
    case BandeEventKind::DragMove:
    case BandeEventKind::DragLeave:
    case BandeEventKind::DragDrop:
    case BandeEventKind::DragFinished:
      DispatchDrag(aEvent);
      break;
    case BandeEventKind::PopupDismissed:
      if (nsIRollupListener* listener = GetActiveRollupListener()) {
        listener->Rollup({});
      }
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

