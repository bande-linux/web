/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef widget_bande_nsClipboard_h
#define widget_bande_nsClipboard_h

#include "nsBaseClipboard.h"
#include "nsTArray.h"

class nsClipboard final : public nsBaseClipboard {
 public:
  nsClipboard();

  NS_DECL_ISUPPORTS_INHERITED

  static void HandleData(const char* aMime, const uint8_t* aData,
                         size_t aLength);
  static void HandleChanged();
  static nsresult Encode(nsITransferable* aTransferable,
                         nsTArray<nsCString>& aMimes,
                         nsTArray<nsCString>& aPayloads);

 protected:
  NS_IMETHOD SetNativeClipboardData(nsITransferable* aTransferable,
                                    ClipboardType aWhichClipboard) override;
  mozilla::Result<nsCOMPtr<nsISupports>, nsresult> GetNativeClipboardData(
      const nsACString& aFlavor, ClipboardType aWhichClipboard,
      uint64_t aThreshold = 0) override;
  void AsyncGetNativeClipboardData(const nsACString& aFlavor,
                                   ClipboardType aWhichClipboard,
                                   GetNativeDataCallback&& aCallback) override;
  nsresult EmptyNativeClipboardData(ClipboardType aWhichClipboard) override;
  mozilla::Result<bool, nsresult> HasNativeClipboardDataMatchingFlavors(
      const nsTArray<nsCString>& aFlavorList,
      ClipboardType aWhichClipboard) override;
  mozilla::Result<int32_t, nsresult> GetNativeClipboardSequenceNumber(
      ClipboardType aWhichClipboard) override;

 private:
  ~nsClipboard() override;

  struct Request {
    nsCString mMime;
    nsCString mFlavor;
    GetNativeDataCallback mCallback;
  };

  nsTArray<nsCString> OfferedTypes() const;
  nsCString WireType(const nsACString& aFlavor) const;
  void Received(const nsACString& aMime, mozilla::Span<const uint8_t> aData);

  nsTArray<Request> mRequests;
  int32_t mSequence = 0;
};

#endif
