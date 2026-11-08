/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef GTKTOOLKIT_H
#define GTKTOOLKIT_H

#include "nsString.h"

class nsGTKToolkit final {
 public:
  nsGTKToolkit() = default;

  static nsGTKToolkit* GetToolkit();

  void SetActivationToken(const nsACString& aToken) {
    mActivationToken = aToken;
  }
  const nsCString& GetActivationToken() const { return mActivationToken; }

  void SetFocusTimestamp(uint32_t aTimestamp) { mFocusTimestamp = aTimestamp; }
  uint32_t GetFocusTimestamp() const { return mFocusTimestamp; }

 private:
  nsCString mActivationToken;
  uint32_t mFocusTimestamp = 0;
};

#endif
