/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "mozilla/widget/NativeMenuSupport.h"

#include "NativeMenu.h"

namespace mozilla::widget {

void NativeMenuSupport::CreateNativeMenuBar(nsIWidget*, dom::Element*) {}

already_AddRefed<NativeMenu> NativeMenuSupport::CreateNativePopupMenu(
    dom::Element*) {
  return nullptr;
}

bool NativeMenuSupport::ShouldUseNativeAnchoredMenus() { return false; }

bool NativeMenuSupport::ShouldUseNativeContextMenus() { return false; }

}  // namespace mozilla::widget
