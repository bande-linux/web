/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef nsBandeIdleService_h__
#define nsBandeIdleService_h__

#include "mozilla/AppShutdown.h"
#include "nsUserIdleService.h"

class nsBandeIdleService final : public nsUserIdleService {
 public:
  NS_INLINE_DECL_REFCOUNTING_INHERITED(nsBandeIdleService, nsUserIdleService)

  static already_AddRefed<nsBandeIdleService> GetInstance();

 private:
  ~nsBandeIdleService() override = default;
};

#endif
