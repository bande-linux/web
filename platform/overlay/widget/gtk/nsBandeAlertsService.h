/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef nsBandeAlertsService_h__
#define nsBandeAlertsService_h__

#include "nsCOMPtr.h"
#include "nsIAlertsService.h"
#include "nsIObserver.h"
#include "nsString.h"
#include "nsTHashMap.h"

class nsBandeAlertsService final : public nsIAlertsService,
                                   public nsIAlertsDoNotDisturb {
 public:
  NS_DECL_ISUPPORTS
  NS_DECL_NSIALERTSSERVICE
  NS_DECL_NSIALERTSDONOTDISTURB

  nsBandeAlertsService() = default;
  nsresult Init();

  static void HandleAction(const nsACString& aId, const nsACString& aAction,
                           const nsACString& aActivationToken);

 private:
  ~nsBandeAlertsService();

  struct Shown {
    nsCOMPtr<nsIAlertNotification> mAlert;
    nsCOMPtr<nsIObserver> mListener;
    nsString mCookie;
    nsString mName;
  };

  void Finish(const nsACString& aId);

  nsTHashMap<nsCStringHashKey, Shown> mShown;
  nsTHashMap<nsStringHashKey, nsCString> mIds;
  uint64_t mNextId = 1;
  bool mSuppressForScreenSharing = false;
};

#endif
