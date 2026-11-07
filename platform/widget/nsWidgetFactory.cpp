/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "nsWidgetFactory.h"

#include "DMABufDevice.h"
#include "mozilla/WidgetUtils.h"
#include "nsAppShell.h"
#include "nsAppShellSingleton.h"
#include "nsLookAndFeel.h"
#include "nsWindow.h"

using namespace mozilla;
using namespace mozilla::widget;

nsresult nsWidgetGtk2ModuleCtor() { return nsAppShellInit(); }

void nsWidgetGtk2ModuleDtor() {
  DMABufDeviceLock::Shutdown();
  WidgetUtils::Shutdown();
  nsLookAndFeel::Shutdown();
  nsAppShellShutdown();
}
