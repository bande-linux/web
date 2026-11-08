/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef widget_bande_BandeWakeLock_h
#define widget_bande_BandeWakeLock_h

#include "nsHashtablesFwd.h"
#include "nsIDOMWakeLockListener.h"
#include "nsTHashSet.h"

class BandeWakeLock final : public nsIDOMMozWakeLockListener {
 public:
  NS_DECL_ISUPPORTS

  nsresult Callback(const nsAString& aTopic, const nsAString& aState) override;

  static void Start();
  static void Stop();

 private:
  ~BandeWakeLock();
  void Update();

  nsTHashSet<nsString> mForeground;
  nsTHashSet<nsString> mBackground;
  uint32_t mFlags = 0;
  uint64_t mInhibitor = 0;
};

#endif
