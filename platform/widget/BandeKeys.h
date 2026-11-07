/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef widget_bande_BandeKeys_h
#define widget_bande_BandeKeys_h

#include <stdint.h>

#include "mozilla/EventForwards.h"
#include "nsString.h"

namespace mozilla::widget {

CodeNameIndex CodeNameFromEvdev(uint32_t aEvdevCode);
KeyNameIndex KeyNameFromKeysym(uint32_t aKeysym);
uint32_t LegacyKeyCode(uint32_t aKeysym, uint32_t aBaseKeysym);
uint32_t CharacterFromKeysym(uint32_t aKeysym);

}  // namespace mozilla::widget

#endif
