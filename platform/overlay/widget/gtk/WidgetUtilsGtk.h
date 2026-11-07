/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef WidgetUtilsGtk_h__
#define WidgetUtilsGtk_h__

#include "nsString.h"
#include "nsTArray.h"
#include "mozilla/MozPromise.h"

namespace mozilla::widget {

class WidgetUtilsGTK {
 public:
  static int32_t IsTouchDeviceSupportPresent();
};

inline bool IsMainWindowTransparent() { return false; }

bool GdkIsWaylandDisplay();
inline bool GdkIsX11Display() { return false; }
inline bool IsXWaylandProtocol() { return false; }

inline const char* GetSnapInstanceName() { return nullptr; }
inline bool IsRunningUnderSnap() { return false; }
inline bool IsRunningUnderFlatpak() { return false; }
inline bool IsPackagedAppFileExists() { return false; }
inline bool IsRunningUnderFlatpakOrSnap() { return false; }

enum class PortalKind {
  FilePicker,
  MimeHandler,
  NativeMessaging,
  Settings,
  Location,
  OpenUri,
};
inline bool ShouldUsePortal(PortalKind) { return true; }

const nsCString& GetDesktopEnvironmentIdentifier();
inline bool IsGnomeDesktopEnvironment() { return false; }
inline bool IsKdeDesktopEnvironment() { return false; }

nsTArray<nsCString> ParseTextURIList(const nsACString& aData);

using FocusRequestPromise = MozPromise<nsCString, bool, false>;
RefPtr<FocusRequestPromise> RequestWaylandFocusPromise();

}  // namespace mozilla::widget

#endif
