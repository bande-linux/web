/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef __nsWindow_h__
#define __nsWindow_h__

#include "BandeHost.h"
#include "CompositorWidget.h"
#include "MozContainer.h"
#include "VsyncDispatcher.h"
#include "mozilla/UniquePtr.h"
#include "mozilla/widget/WaylandSurfaceLock.h"
#include "mozilla/widget/WindowSurface.h"
#include "mozilla/widget/WindowSurfaceProvider.h"
#include "nsIWidget.h"

#ifdef MOZ_LOGGING
#  undef LOG
#  undef LOGVERBOSE

#  include "mozilla/Logging.h"
#  include "nsTArray.h"
#  include "Units.h"

extern mozilla::LazyLogModule gWidgetLog;
extern mozilla::LazyLogModule gWidgetDragLog;
extern mozilla::LazyLogModule gWidgetPopupLog;
extern mozilla::LazyLogModule gWidgetVsync;
extern mozilla::LazyLogModule gWidgetWaylandLog;

#  define LOG_WIN(win, str, ...)                           \
    MOZ_LOG(win->IsPopup() ? gWidgetPopupLog : gWidgetLog, \
            mozilla::LogLevel::Debug,                      \
            ("%s: " str, win->GetDebugTag().get(), ##__VA_ARGS__))
#  define LOG(...) LOG_WIN(this, __VA_ARGS__)
#  define LOGVERBOSE(str, ...)                        \
    MOZ_LOG(IsPopup() ? gWidgetPopupLog : gWidgetLog, \
            mozilla::LogLevel::Verbose,               \
            ("%s: " str, GetDebugTag().get(), ##__VA_ARGS__))
#  define LOGW(...) MOZ_LOG(gWidgetLog, mozilla::LogLevel::Debug, (__VA_ARGS__))
#  define LOGDRAG(...) \
    MOZ_LOG(gWidgetDragLog, mozilla::LogLevel::Debug, (__VA_ARGS__))
#  define LOG_POPUP(...) \
    MOZ_LOG(gWidgetPopupLog, mozilla::LogLevel::Debug, (__VA_ARGS__))
#  define LOG_VSYNC(str, ...)                       \
    MOZ_LOG(gWidgetVsync, mozilla::LogLevel::Debug, \
            ("%s: " str, GetDebugTag().get(), ##__VA_ARGS__))
#  define LOG_ENABLED()                                         \
    (MOZ_LOG_TEST(gWidgetPopupLog, mozilla::LogLevel::Debug) || \
     MOZ_LOG_TEST(gWidgetLog, mozilla::LogLevel::Debug))
#  define LOG_ENABLED_VERBOSE()                                   \
    (MOZ_LOG_TEST(gWidgetPopupLog, mozilla::LogLevel::Verbose) || \
     MOZ_LOG_TEST(gWidgetLog, mozilla::LogLevel::Verbose))
#  define LOG_WAYLAND(...) \
    MOZ_LOG(gWidgetWaylandLog, mozilla::LogLevel::Debug, (__VA_ARGS__))

#else

#  define LOG(...)
#  define LOG_WIN(...)
#  define LOGVERBOSE(...)
#  define LOGW(...)
#  define LOGDRAG(...)
#  define LOG_POPUP(...)
#  define LOG_ENABLED() false

#endif /* MOZ_LOGGING */

#if defined(MOZ_WAYLAND) && !defined(MOZ_X11)
typedef uintptr_t Window;
#endif

namespace mozilla {
class WaylandVsyncSource;
namespace widget {
class PlatformCompositorWidgetDelegate;
}  // namespace widget
}  // namespace mozilla

extern bool gUseStableRounding;

class nsWindow final : public nsIWidget {
 public:
  using LayoutDeviceIntRegion = mozilla::LayoutDeviceIntRegion;

  nsWindow();

  NS_INLINE_DECL_REFCOUNTING_INHERITED(nsWindow, nsIWidget)

  static void HandleBandeEvent(const mozilla::widget::BandeEvent& aEvent);
  static nsWindow* FromWidget(nsIWidget* aWidget) {
    if (aWidget && aWidget->IsNativeWidget()) {
      return static_cast<nsWindow*>(aWidget);
    }
    return nullptr;
  }
  static nsWindow* FromWidget(nsWindow*) = delete;

  nsresult Create(nsIWidget* aParent, const LayoutDeviceIntRect& aRect,
                  const mozilla::widget::InitData& aInitData) override;
  using nsIWidget::Create;
  void Destroy() override;

  double GetDefaultScaleInternal() override;
  mozilla::DesktopToLayoutDeviceScale GetDesktopToDeviceScale() const override;

  void Show(bool aState) override;
  bool IsVisible() const override { return mVisible; }
  void Move(const mozilla::DesktopPoint&) override;
  void Resize(const mozilla::DesktopSize&, bool aRepaint) override;
  void Resize(const mozilla::DesktopRect&, bool aRepaint) override;
  nsSizeMode SizeMode() override { return mSizeMode; }
  void SetSizeMode(nsSizeMode aMode) override;
  nsresult MakeFullScreen(bool aFullScreen) override;
  void Enable(bool aState) override { mEnabled = aState; }
  bool IsEnabled() const override { return mEnabled; }
  void SetFocus(Raise, mozilla::dom::CallerType) override {}
  LayoutDeviceIntRect GetBounds() override { return mBounds; }
  LayoutDeviceIntSize GetClientSize() override { return mBounds.Size(); }
  void Invalidate(const LayoutDeviceIntRect& aRect) override;
  void* GetNativeData(uint32_t aDataType) override;
  nsresult SetTitle(const nsAString& aTitle) override;
  void SetWindowClass(const nsAString& aType, const nsAString& aClass,
                      const nsAString& aName) override;
  LayoutDeviceIntPoint WidgetToScreenOffset() override;
  void CaptureRollupEvents(bool aDoCapture) override {}
  void SetCursor(const Cursor& aCursor) override;
  void SetInputContext(const InputContext& aContext,
                       const InputContextAction&) override {
    mInputContext = aContext;
  }
  InputContext GetInputContext() override { return mInputContext; }
  TextEventDispatcherListener* GetNativeTextEventDispatcherListener() override;
  MOZ_CAN_RUN_SCRIPT bool GetEditCommands(
      mozilla::NativeKeyBindingsType aType,
      const mozilla::WidgetKeyboardEvent& aEvent,
      nsTArray<mozilla::CommandInt>& aCommands) override;

  void GetCompositorWidgetInitData(
      mozilla::widget::CompositorWidgetInitData* aInitData) override;
  void SetCompositorWidgetDelegate(CompositorWidgetDelegate* aDelegate) override;
  void CreateCompositorVsyncDispatcher() override;
  RefPtr<mozilla::VsyncDispatcher> GetVsyncDispatcher() override;
  bool WidgetTypeSupportsAcceleration() override;
  bool WidgetTypeSupportsNativeCompositing() override;

  MozContainer* GetMozContainer() { return &mContainer; }
  mozilla::widget::WaylandSurface* GetWaylandSurface() {
    return mContainer.mSurface;
  }
  mozilla::UniquePtr<mozilla::widget::WaylandSurfaceLock> LockSurface();
  LayoutDeviceIntRegion GetOpaqueRegion() const { return mOpaqueRegion; }
  void UpdateOpaqueRegion(const LayoutDeviceIntRegion& aRegion) override;
  bool IsPopup() const { return mWindowType == WindowType::Popup; }
  bool IsDragPopup() const { return false; }
  bool IsDestroyed() const { return mIsDestroyed; }

 private:
  ~nsWindow() override;

  double FractionalScaleFactor() const;
  LayoutDeviceIntPoint ToDevice(double aX, double aY) const;
  void MapToplevel();
  void MapPopup();
  nsWindow* ParentWindow() const;
  LayoutDeviceIntPoint ParentOrigin() const;
  bool RolledUpOnPress();
  bool MapSurfaceLocked(mozilla::widget::WaylandSurfaceLock& aLock);
  void UnmapToplevel();
  void RefreshBounds(uint32_t aWidth, uint32_t aHeight);
  void Paint();
  void RefreshScale();
  void OnBandeEvent(const mozilla::widget::BandeEvent& aEvent);
  void DispatchPointer(const mozilla::widget::BandeEvent& aEvent);
  void DispatchScroll(const mozilla::widget::BandeEvent& aEvent);
  void DispatchKey(const mozilla::widget::BandeEvent& aEvent);
  void DispatchFocus(bool aFocused);
  void DispatchToplevelState(uint32_t aState);
  mozilla::Modifiers ModifiersOf(uint32_t aBits) const;

  uint64_t mBandeWindow = 0;
  bool mGrabs = false;
  bool mActivated = false;
  MozContainer mContainer;
  LayoutDeviceIntRect mBounds;
  LayoutDeviceIntRegion mOpaqueRegion;
  nsSizeMode mSizeMode = nsSizeMode_Normal;
  nsCString mTitle;
  nsCString mAppId;
  InputContext mInputContext;
  mozilla::widget::PlatformCompositorWidgetDelegate* mCompositorWidgetDelegate =
      nullptr;
  RefPtr<mozilla::WaylandVsyncSource> mVsyncSource;
  RefPtr<mozilla::VsyncDispatcher> mVsyncDispatcher;
  uint16_t mPressedButtons = 0;
  bool mVisible = false;
  bool mEnabled = true;
  bool mIsDestroyed = false;
  bool mConfiguredClearColor = false;
};

#endif
