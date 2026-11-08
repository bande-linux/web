/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "nsLookAndFeel.h"

nsresult nsLookAndFeel::NativeGetInt(IntID aID, int32_t& aResult) {
  switch (aID) {
    case IntID::CaretBlinkTime:
      aResult = 0;
      return NS_OK;
    case IntID::CaretBlinkCount:
      aResult = -1;
      return NS_OK;
    default:
      return HeadlessLookAndFeel::NativeGetInt(aID, aResult);
  }
}
