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

namespace mozilla::widget {
class PlatformCompositorWidgetDelegate;
class WaylandVsyncSource;
}  // namespace mozilla::widget

class nsWindow final : public nsIWidget {
 public:
  using LayoutDeviceIntRegion = mozilla::LayoutDeviceIntRegion;

  nsWindow();

  NS_INLINE_DECL_REFCOUNTING_INHERITED(nsWindow, nsIWidget)

  static void HandleBandeEvent(const mozilla::widget::BandeEvent& aEvent);

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
  LayoutDeviceIntPoint WidgetToScreenOffset() override { return {}; }
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
  MozContainer mContainer;
  LayoutDeviceIntRect mBounds;
  LayoutDeviceIntRegion mOpaqueRegion;
  nsSizeMode mSizeMode = nsSizeMode_Normal;
  nsCString mTitle;
  InputContext mInputContext;
  mozilla::widget::PlatformCompositorWidgetDelegate* mCompositorWidgetDelegate =
      nullptr;
  RefPtr<mozilla::widget::WaylandVsyncSource> mVsyncSource;
  RefPtr<mozilla::VsyncDispatcher> mVsyncDispatcher;
  mozilla::UniquePtr<mozilla::Mutex> mCompositorVsyncDispatcherLock;
  uint16_t mPressedButtons = 0;
  bool mVisible = false;
  bool mEnabled = true;
  bool mIsDestroyed = false;
  bool mConfiguredClearColor = false;
};

#endif
