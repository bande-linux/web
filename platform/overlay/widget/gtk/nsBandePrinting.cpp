/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "nsBandePrinting.h"

#include "BandeHost.h"
#include "mozilla/Logging.h"
#include "mozilla/dom/Promise.h"
#include "mozilla/gfx/PrintTargetPDF.h"
#include <sys/stat.h>

#include "nsAppDirectoryServiceDefs.h"
#include "nsDirectoryServiceDefs.h"
#include "nsComponentManagerUtils.h"
#include "nsDirectoryServiceUtils.h"
#include "nsIFileStreams.h"
#include "nsIGlobalObject.h"
#include "nsPIDOMWindow.h"
#include "nsPrintSettingsImpl.h"
#include "xpcpublic.h"

using namespace mozilla;
using namespace mozilla::gfx;
using namespace mozilla::widget;

static LazyLogModule sPrintLog("BandePrint");

NS_IMPL_ISUPPORTS(nsDeviceContextSpecBande, nsIDeviceContextSpec)

NS_IMETHODIMP
nsDeviceContextSpecBande::Init(nsIPrintSettings* aPS, bool aIsPrintPreview) {
  mPrintSettings = aPS;
  return NS_OK;
}

already_AddRefed<PrintTarget> nsDeviceContextSpecBande::MakePrintTarget() {
  double width, height;
  mPrintSettings->GetEffectiveSheetSize(&width, &height);
  width /= TWIPS_PER_POINT_FLOAT;
  height /= TWIPS_PER_POINT_FLOAT;

  nsCOMPtr<nsIOutputStream> stream;
  if (mPrintSettings->GetOutputDestination() ==
      nsIPrintSettings::kOutputDestinationStream) {
    mPrintSettings->GetOutputStream(getter_AddRefs(stream));
  } else {
    if (NS_FAILED(NS_GetSpecialDirectory(NS_OS_TEMP_DIR,
                                         getter_AddRefs(mSpoolFile))) ||
        NS_FAILED(mSpoolFile->AppendNative("bande-print.pdf"_ns)) ||
        NS_FAILED(mSpoolFile->CreateUnique(nsIFile::NORMAL_FILE_TYPE, 0600))) {
      return nullptr;
    }
    nsCOMPtr<nsIFileOutputStream> fileStream =
        do_CreateInstance("@mozilla.org/network/file-output-stream;1");
    if (!fileStream || NS_FAILED(fileStream->Init(mSpoolFile, -1, -1, 0))) {
      return nullptr;
    }
    stream = fileStream;
  }
  return PrintTargetPDF::CreateOrNull(stream, IntSize::Ceil(width, height));
}

NS_IMETHODIMP
nsDeviceContextSpecBande::BeginDocument(const nsAString& aTitle,
                                        const nsAString& aPrintToFileName,
                                        uint64_t aBrowsingContextId,
                                        int32_t aStartPage, int32_t aEndPage) {
  CopyUTF16toUTF8(aTitle, mTitle);
  return NS_OK;
}

RefPtr<PrintEndDocumentPromise> nsDeviceContextSpecBande::EndDocument() {
  MOZ_LOG(sPrintLog, LogLevel::Debug,
          ("end document, destination %d, spool %p",
           int(mPrintSettings->GetOutputDestination()), mSpoolFile.get()));
  switch (mPrintSettings->GetOutputDestination()) {
    case nsIPrintSettings::kOutputDestinationPrinter: {
      nsAutoCString path;
      if (!mSpoolFile || NS_FAILED(mSpoolFile->GetNativePath(path)) ||
          !BandeHost::IsActive()) {
        return PrintEndDocumentPromise::CreateAndReject(NS_ERROR_FAILURE,
                                                        __func__);
      }
      BandeHost::PrintPdf(mTitle.get(), path.get());
      mSpoolFile = nullptr;
      break;
    }
    case nsIPrintSettings::kOutputDestinationFile: {
      nsString targetPath;
      nsCOMPtr<nsIFile> destFile;
      mPrintSettings->GetToFileName(targetPath);
      nsresult rv = NS_NewLocalFile(targetPath, getter_AddRefs(destFile));
      if (NS_FAILED(rv)) {
        return PrintEndDocumentPromise::CreateAndReject(rv, __func__);
      }
      return nsIDeviceContextSpec::EndDocumentAsync(
          __func__,
          [destFile = std::move(destFile),
           spoolFile = std::move(mSpoolFile)]() -> nsresult {
            nsAutoString leaf;
            nsresult rv = destFile->GetLeafName(leaf);
            NS_ENSURE_SUCCESS(rv, rv);
            nsCOMPtr<nsIFile> dir;
            rv = destFile->GetParent(getter_AddRefs(dir));
            NS_ENSURE_SUCCESS(rv, rv);
            rv = spoolFile->MoveTo(dir, leaf);
            NS_ENSURE_SUCCESS(rv, rv);
            mode_t mask = umask(0);
            umask(mask);
            destFile->SetPermissions(0666 & ~mask);
            return NS_OK;
          });
    }
    case nsIPrintSettings::kOutputDestinationStream:
      break;
  }
  return PrintEndDocumentPromise::CreateAndResolve(true, __func__);
}

NS_IMPL_ISUPPORTS(nsPrintDialogServiceBande, nsIPrintDialogService)

NS_IMETHODIMP
nsPrintDialogServiceBande::Init() { return NS_OK; }

static nsresult ResolvedPromise(JSContext* aCx, nsresult aResult,
                                dom::Promise** aPromise) {
  nsIGlobalObject* global = xpc::CurrentNativeGlobal(aCx);
  if (!global) {
    return NS_ERROR_FAILURE;
  }
  ErrorResult error;
  RefPtr<dom::Promise> promise = dom::Promise::Create(global, error);
  if (error.Failed()) {
    return error.StealNSResult();
  }
  if (NS_SUCCEEDED(aResult)) {
    promise->MaybeResolveWithUndefined();
  } else {
    promise->MaybeReject(aResult);
  }
  promise.forget(aPromise);
  return NS_OK;
}

NS_IMETHODIMP
nsPrintDialogServiceBande::ShowPrintDialog(mozIDOMWindowProxy* aParent,
                                           bool aHaveSelection,
                                           nsIPrintSettings* aSettings,
                                           JSContext* aCx,
                                           dom::Promise** aPromise) {
  MOZ_LOG(sPrintLog, LogLevel::Debug, ("print dialog asked"));
  aSettings->SetOutputDestination(nsIPrintSettings::kOutputDestinationPrinter);
  aSettings->SetPrintSilent(true);
  return ResolvedPromise(aCx, NS_OK, aPromise);
}

NS_IMETHODIMP
nsPrintDialogServiceBande::ShowPageSetupDialog(mozIDOMWindowProxy* aParent,
                                               nsIPrintSettings* aSettings,
                                               JSContext* aCx,
                                               dom::Promise** aPromise) {
  return ResolvedPromise(aCx, NS_ERROR_NOT_IMPLEMENTED, aPromise);
}

nsresult nsPrintSettingsServiceBande::_CreatePrintSettings(
    nsIPrintSettings** aResult) {
  RefPtr<nsPrintSettings> settings = new nsPrintSettings();
  settings.forget(aResult);
  return NS_OK;
}

already_AddRefed<nsIPrintSettings> CreatePlatformPrintSettings(
    const PrintSettingsInitializer& aSettings) {
  auto settings = MakeRefPtr<nsPrintSettings>();
  settings->InitWithInitializer(aSettings);
  return settings.forget();
}
