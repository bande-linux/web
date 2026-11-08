/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "ScreenHelperBande.h"

#include <atomic>
#include <cmath>

#include "mozilla/widget/ScreenManager.h"
#include "mozilla/widget/nsWaylandDisplay.h"
#include "nsThreadUtils.h"
#include "nsXULAppAPI.h"

using namespace mozilla;
using namespace mozilla::widget;

static double sScale = 0.0;
static std::atomic<double> sDefaultScale{0.0};
static bool sStarted = false;

void ScreenHelperBande::EnsureStarted() {
  if (sStarted || !XRE_IsParentProcess()) {
    return;
  }
  sStarted = true;
  if (nsWaylandDisplay* display = WaylandDisplayGet()) {
    display->RequestAsyncRoundtrip();
    display->WaitForAsyncRoundtrips();
    display->RequestAsyncRoundtrip();
    display->WaitForAsyncRoundtrips();
  }
  Refresh();
}

double ScreenHelperBande::DefaultScale() {
  if (NS_IsMainThread()) {
    EnsureStarted();
  }
  const double scale = sDefaultScale;
  return scale > 0.0 ? scale : 1.0;
}

void ScreenHelperBande::SetScale(double aScale) {
  if (aScale > 0.0 && aScale != sScale) {
    sScale = aScale;
    Refresh();
  }
}

void ScreenHelperBande::Refresh() {
  if (!sStarted) {
    return;
  }
  AutoTArray<RefPtr<Screen>, 4> screens;
  double defaultScale = 0.0;
  if (nsWaylandDisplay* display = WaylandDisplayGet()) {
    for (const auto& monitor : display->GetMonitors()) {
      if (monitor->pendingChanges || !monitor->pixelWidth) {
        continue;
      }
      const bool logical = monitor->logicalWidth > 0 && monitor->logicalHeight > 0;
      const double scale =
          logical ? std::sqrt(double(monitor->pixelWidth) * monitor->pixelHeight /
                              (double(monitor->logicalWidth) * monitor->logicalHeight))
          : sScale > 0.0 ? sScale
                         : double(monitor->scale);
      const LayoutDeviceIntRect rect =
          logical ? LayoutDeviceIntRect(
                        int32_t(std::round(monitor->logicalX * scale)),
                        int32_t(std::round(monitor->logicalY * scale)),
                        int32_t(std::round(monitor->logicalWidth * scale)),
                        int32_t(std::round(monitor->logicalHeight * scale)))
                  : LayoutDeviceIntRect(int32_t(monitor->x * scale),
                                        int32_t(monitor->y * scale),
                                        monitor->pixelWidth, monitor->pixelHeight);
      if (defaultScale <= 0.0) {
        defaultScale = scale;
      }
      const float refresh = monitor->refresh ? monitor->refresh / 1000.0f : 60.0f;
      screens.AppendElement(MakeRefPtr<Screen>(
          rect, rect, 24, 24, uint32_t(refresh), DesktopToLayoutDeviceScale(scale),
          CSSToLayoutDeviceScale(scale), float(96.0 * scale),
          Screen::IsPseudoDisplay::No, Screen::IsHDR::No, 80.0f, 1000.0f));
    }
  }
  sDefaultScale = defaultScale;
  if (screens.IsEmpty()) {
    const LayoutDeviceIntRect rect(0, 0, 1920, 1200);
    screens.AppendElement(MakeRefPtr<Screen>(
        rect, rect, 24, 24, 60, DesktopToLayoutDeviceScale(),
        CSSToLayoutDeviceScale(), 96.0f, Screen::IsPseudoDisplay::No,
        Screen::IsHDR::No, 80.0f, 1000.0f));
  }
  ScreenManager::Refresh(std::move(screens));
}
