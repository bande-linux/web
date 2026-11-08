/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef nsLookAndFeel_h__
#define nsLookAndFeel_h__

#include "HeadlessLookAndFeel.h"

class nsLookAndFeel final : public mozilla::widget::HeadlessLookAndFeel {
 public:
  nsLookAndFeel() = default;

  nsresult NativeGetInt(IntID aID, int32_t& aResult) override;
};

#endif
