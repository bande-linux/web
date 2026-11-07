/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef widget_bande_BandeTimers_h
#define widget_bande_BandeTimers_h

#include <stdint.h>

namespace mozilla::widget {

using BandeTimerCallback = bool (*)(void* aData);

uint32_t BandeTimeoutAdd(uint32_t aMilliseconds, BandeTimerCallback aCallback,
                         void* aData);
void BandeTimeoutRemove(uint32_t aTimer);

}  // namespace mozilla::widget

#endif
