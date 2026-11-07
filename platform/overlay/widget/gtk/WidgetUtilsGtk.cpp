/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "WidgetUtilsGtk.h"

#include "BandeHost.h"
#include "gfxPlatform.h"
#include "mozilla/WidgetUtils.h"

namespace mozilla {
namespace widget {

int32_t WidgetUtilsGTK::IsTouchDeviceSupportPresent() { return 0; }

bool GdkIsWaylandDisplay() { return !gfxPlatform::IsHeadless(); }

const nsCString& GetDesktopEnvironmentIdentifier() {
  static const nsCString sIdentifier("bande"_ns);
  return sIdentifier;
}

nsTArray<nsCString> ParseTextURIList(const nsACString& aData) {
  nsTArray<nsCString> uris;
  for (const auto& line : aData.Split('\n')) {
    nsAutoCString uri(line);
    uri.Trim("\r ");
    if (!uri.IsEmpty() && uri.First() != '#') {
      uris.AppendElement(uri);
    }
  }
  return uris;
}

RefPtr<FocusRequestPromise> RequestWaylandFocusPromise() {
  return FocusRequestPromise::CreateAndReject(false, __func__);
}

}  // namespace widget
}  // namespace mozilla
