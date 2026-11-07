/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef widget_bande_BandeFdWatch_h
#define widget_bande_BandeFdWatch_h

#include <stdint.h>

namespace mozilla::widget {

using BandeFdCallback = bool (*)(int aFd, void* aData);

uint32_t BandeFdWatchAdd(int aFd, BandeFdCallback aCallback, void* aData);
void BandeFdWatchRemove(uint32_t aWatch);
void BandeFdWatchFire(int aFd);

}  // namespace mozilla::widget

#endif
