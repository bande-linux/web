/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef nsAppShell_h__
#define nsAppShell_h__

#include "nsBaseAppShell.h"

class nsAppShell final : public nsBaseAppShell {
 public:
  nsAppShell() = default;

  nsresult Init();

 private:
  ~nsAppShell() override;

  void ScheduleNativeEventCallback() override;
  bool ProcessNextNativeEvent(bool aMayWait) override;
  bool ProcessProcessLocal(bool aMayWait);

  int mLocalPipe[2] = {-1, -1};
  int mWakeWrite = -1;
};

#endif
