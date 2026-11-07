/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef widget_bande_nsDragService_h
#define widget_bande_nsDragService_h

#include "nsBaseDragService.h"
#include "nsCOMPtr.h"
#include "nsITransferable.h"

class nsWindow;

class nsDragSession final : public nsBaseDragSession {
 public:
  NS_IMETHOD GetData(nsITransferable* aTransferable, uint32_t aItem) override;
  NS_IMETHOD GetNumDropItems(uint32_t* aNumItems) override;
  NS_IMETHOD IsDataFlavorSupported(const char* aDataFlavor,
                                   bool* aResult) override;
  MOZ_CAN_RUN_SCRIPT nsresult EndDragSessionImpl(bool aDoneDrag,
                                                 uint32_t aKeyModifiers) override;

  void SetIncoming(const nsACString& aText, const nsACString& aUris,
                   bool aHasText);

 protected:
  ~nsDragSession() override = default;

  MOZ_CAN_RUN_SCRIPT nsresult InvokeDragSessionImpl(
      nsIWidget* aWidget, nsIArray* aTransferables,
      const mozilla::Maybe<mozilla::CSSIntRegion>& aRegion,
      uint32_t aActionType) override;

 private:
  nsCOMPtr<nsITransferable> mTransferable;
};

class nsDragService final : public nsBaseDragService {
 public:
  static already_AddRefed<nsDragService> GetInstance();

 protected:
  already_AddRefed<nsIDragSession> CreateDragSession() override;
};

#endif
