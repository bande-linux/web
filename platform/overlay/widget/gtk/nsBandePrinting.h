/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef widget_bande_nsBandePrinting_h
#define widget_bande_nsBandePrinting_h

#include "nsCOMPtr.h"
#include "nsIDeviceContextSpec.h"
#include "nsIFile.h"
#include "nsIPrintDialogService.h"
#include "nsPrintSettingsService.h"
#include "nsString.h"

class nsDeviceContextSpecBande final : public nsIDeviceContextSpec {
 public:
  NS_DECL_ISUPPORTS

  already_AddRefed<PrintTarget> MakePrintTarget() final;
  NS_IMETHOD Init(nsIPrintSettings* aPS, bool aIsPrintPreview) override;
  NS_IMETHOD BeginDocument(const nsAString& aTitle,
                           const nsAString& aPrintToFileName,
                           uint64_t aBrowsingContextId, int32_t aStartPage,
                           int32_t aEndPage) override;
  RefPtr<mozilla::gfx::PrintEndDocumentPromise> EndDocument() override;
  NS_IMETHOD BeginPage(const IntSize& aSizeInPoints) override { return NS_OK; }
  NS_IMETHOD EndPage() override { return NS_OK; }

 private:
  ~nsDeviceContextSpecBande() = default;

  nsCOMPtr<nsIFile> mSpoolFile;
  nsCString mTitle;
};

class nsPrintDialogServiceBande final : public nsIPrintDialogService {
 public:
  NS_DECL_ISUPPORTS
  NS_DECL_NSIPRINTDIALOGSERVICE

 private:
  ~nsPrintDialogServiceBande() = default;
};

class nsPrintSettingsServiceBande final : public nsPrintSettingsService {
 protected:
  nsresult _CreatePrintSettings(nsIPrintSettings** aResult) override;
};

#endif
