/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "nsDragService.h"

#include "BandeHost.h"
#include "mozilla/ClearOnShutdown.h"
#include "mozilla/Logging.h"
#include "mozilla/StaticPtr.h"
#include "nsArrayUtils.h"
#include "nsClipboard.h"
#include "nsComponentManagerUtils.h"
#include "nsIFile.h"
#include "nsNetUtil.h"
#include "nsSupportsPrimitives.h"
#include "nsWindow.h"

using namespace mozilla;
using namespace mozilla::widget;

static StaticRefPtr<nsDragService> sDragService;
already_AddRefed<nsDragService> nsDragService::GetInstance() {
  if (!sDragService) {
    sDragService = new nsDragService();
    ClearOnShutdown(&sDragService);
  }
  return do_AddRef(sDragService.get());
}

already_AddRefed<nsIDragSession> nsDragService::CreateDragSession() {
  return MakeAndAddRef<nsDragSession>();
}

static uint32_t BandeActions(uint32_t aActionType) {
  uint32_t actions = 0;
  if (aActionType & nsIDragService::DRAGDROP_ACTION_COPY) {
    actions |= 1;
  }
  if (aActionType & nsIDragService::DRAGDROP_ACTION_MOVE) {
    actions |= 2;
  }
  return actions ? actions : 1;
}

nsresult nsDragSession::InvokeDragSessionImpl(
    nsIWidget* aWidget, nsIArray* aTransferables,
    const Maybe<CSSIntRegion>& aRegion, uint32_t aActionType) {
  nsCOMPtr<nsITransferable> transferable =
      do_QueryElementAt(aTransferables, 0);
  nsWindow* window = nsWindow::FromWidget(aWidget);
  if (!transferable || !window || !window->BandeWindow()) {
    return NS_ERROR_FAILURE;
  }
  nsTArray<nsCString> mimes;
  nsTArray<nsCString> payloads;
  nsresult rv = nsClipboard::Encode(transferable, mimes, payloads);
  NS_ENSURE_SUCCESS(rv, rv);
  nsTArray<const char*> mimePointers;
  nsTArray<const uint8_t*> dataPointers;
  nsTArray<size_t> lengths;
  for (size_t i = 0; i < mimes.Length(); i++) {
    mimePointers.AppendElement(mimes[i].get());
    dataPointers.AppendElement(
        reinterpret_cast<const uint8_t*>(payloads[i].get()));
    lengths.AppendElement(payloads[i].Length());
  }
  mTransferable = transferable;
  if (!BandeHost::StartDrag(window->BandeWindow(), BandeActions(aActionType),
                            mimes.Length(), mimePointers.Elements(),
                            dataPointers.Elements(), lengths.Elements())) {
    mTransferable = nullptr;
    return NS_ERROR_FAILURE;
  }
  return NS_OK;
}

void nsDragSession::SetIncoming(const nsACString& aText,
                                const nsACString& aUris, bool aHasText) {
  nsCOMPtr<nsITransferable> transferable =
      do_CreateInstance("@mozilla.org/widget/transferable;1");
  if (!transferable) {
    return;
  }
  transferable->Init(nullptr);
  nsAutoCString firstUri;
  for (const auto& line : aUris.Split('\n')) {
    if (!line.IsEmpty()) {
      firstUri.Assign(line);
      break;
    }
  }
  if (!firstUri.IsEmpty()) {
    nsCOMPtr<nsIFile> file;
    if (StringBeginsWith(firstUri, "file:"_ns) &&
        NS_SUCCEEDED(NS_GetFileFromURLSpec(firstUri, getter_AddRefs(file)))) {
      transferable->AddDataFlavor(kFileMime);
      transferable->SetTransferData(kFileMime, file);
    }
    auto url = MakeRefPtr<nsSupportsString>();
    url->SetData(NS_ConvertUTF8toUTF16(firstUri + "\n"_ns + firstUri));
    transferable->AddDataFlavor(kURLMime);
    transferable->SetTransferData(kURLMime, url);
  }
  if (aHasText || !aText.IsEmpty() || !firstUri.IsEmpty()) {
    auto text = MakeRefPtr<nsSupportsString>();
    text->SetData(NS_ConvertUTF8toUTF16(aText.IsEmpty() ? firstUri
                                                        : nsCString(aText)));
    transferable->AddDataFlavor(kTextMime);
    transferable->SetTransferData(kTextMime, text);
  }
  mTransferable = transferable;
}

NS_IMETHODIMP
nsDragSession::GetData(nsITransferable* aTransferable, uint32_t aItem) {
  if (!aTransferable || !mTransferable) {
    return NS_ERROR_FAILURE;
  }
  nsTArray<nsCString> flavors;
  nsresult rv = aTransferable->FlavorsTransferableCanImport(flavors);
  NS_ENSURE_SUCCESS(rv, rv);
  for (const auto& flavor : flavors) {
    nsCOMPtr<nsISupports> data;
    if (NS_SUCCEEDED(mTransferable->GetTransferData(flavor.get(),
                                                    getter_AddRefs(data))) &&
        NS_SUCCEEDED(aTransferable->SetTransferData(flavor.get(), data))) {
      return NS_OK;
    }
  }
  return NS_ERROR_FAILURE;
}

NS_IMETHODIMP
nsDragSession::GetNumDropItems(uint32_t* aNumItems) {
  *aNumItems = mTransferable ? 1 : 0;
  return NS_OK;
}

NS_IMETHODIMP
nsDragSession::IsDataFlavorSupported(const char* aDataFlavor, bool* aResult) {
  *aResult = false;
  if (!mTransferable) {
    return NS_OK;
  }
  nsTArray<nsCString> flavors;
  if (NS_SUCCEEDED(mTransferable->FlavorsTransferableCanExport(flavors))) {
    *aResult = flavors.Contains(nsDependentCString(aDataFlavor));
  }
  return NS_OK;
}

nsresult nsDragSession::EndDragSessionImpl(bool aDoneDrag,
                                           uint32_t aKeyModifiers) {
  nsresult rv = nsBaseDragSession::EndDragSessionImpl(aDoneDrag, aKeyModifiers);
  mTransferable = nullptr;
  return rv;
}
