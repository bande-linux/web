/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "nsClipboard.h"

#include "BandeHost.h"
#include "imgITools.h"
#include "imgIContainer.h"
#include "mozilla/SpinEventLoopUntil.h"
#include "mozilla/StaticPrefs_clipboard.h"
#include "nsComponentManagerUtils.h"
#include "nsContentUtils.h"
#include "nsIFile.h"
#include "nsIInputStream.h"
#include "nsIURI.h"
#include "nsITransferable.h"
#include "nsNetUtil.h"
#include "nsPrimitiveHelpers.h"
#include "nsStreamUtils.h"
#include "nsStringStream.h"
#include "nsSupportsPrimitives.h"
#include "nsWidgetsCID.h"

using namespace mozilla;
using namespace mozilla::widget;

static const char kUtf8TextWire[] = "text/plain;charset=utf-8";
static const char kUriListWire[] = "text/uri-list";
static const char* const kTextWires[] = {kUtf8TextWire, "UTF8_STRING",
                                         "text/plain"};

static nsClipboard* sInstance = nullptr;

NS_IMPL_ISUPPORTS_INHERITED0(nsClipboard, nsBaseClipboard)

nsClipboard::nsClipboard()
    : nsBaseClipboard(mozilla::dom::ClipboardCapabilities(
          true /* supportsSelectionClipboard */,
          false /* supportsFindClipboard */,
          false /* supportsSelectionCache */)) {
  sInstance = this;
}

nsClipboard::~nsClipboard() {
  if (sInstance == this) {
    sInstance = nullptr;
  }
}

static bool IsImageFlavor(const nsACString& aFlavor) {
  return aFlavor.EqualsLiteral(kPNGImageMime) ||
         aFlavor.EqualsLiteral(kJPEGImageMime) ||
         aFlavor.EqualsLiteral(kJPGImageMime) ||
         aFlavor.EqualsLiteral(kGIFImageMime);
}

static bool EncodePng(nsISupports* aImage, nsCString& aOut) {
  nsCOMPtr<imgIContainer> image = do_QueryInterface(aImage);
  nsCOMPtr<imgITools> tools = do_CreateInstance("@mozilla.org/image/tools;1");
  if (!image || !tools) {
    return false;
  }
  nsCOMPtr<nsIInputStream> stream;
  if (NS_FAILED(tools->EncodeImage(image, nsLiteralCString(kPNGImageMime),
                                   u""_ns, getter_AddRefs(stream)))) {
    return false;
  }
  return NS_SUCCEEDED(NS_ConsumeStream(stream, UINT32_MAX, aOut));
}

NS_IMETHODIMP
nsClipboard::SetNativeClipboardData(nsITransferable* aTransferable,
                                    ClipboardType aWhichClipboard) {
  if (!BandeHost::IsActive()) {
    return NS_ERROR_NOT_AVAILABLE;
  }
  nsTArray<nsCString> flavors;
  nsresult rv = aTransferable->FlavorsTransferableCanExport(flavors);
  NS_ENSURE_SUCCESS(rv, rv);

  nsTArray<nsCString> mimes;
  nsTArray<nsCString> payloads;
  bool imageDone = false;
  for (const nsCString& flavor : flavors) {
    nsCOMPtr<nsISupports> item;
    if (NS_FAILED(aTransferable->GetTransferData(flavor.get(),
                                                 getter_AddRefs(item))) ||
        !item) {
      continue;
    }
    if (flavor.EqualsLiteral(kTextMime) || flavor.EqualsLiteral(kHTMLMime)) {
      nsCOMPtr<nsISupportsString> text = do_QueryInterface(item);
      if (!text) {
        continue;
      }
      nsAutoString value;
      text->GetData(value);
      mimes.AppendElement(flavor.EqualsLiteral(kTextMime)
                              ? nsLiteralCString(kUtf8TextWire)
                              : flavor);
      payloads.AppendElement(NS_ConvertUTF16toUTF8(value));
      continue;
    }
    if (nsContentUtils::IsFlavorImage(flavor)) {
      nsCString png;
      if (!imageDone && EncodePng(item, png)) {
        mimes.AppendElement(nsLiteralCString(kPNGImageMime));
        payloads.AppendElement(std::move(png));
        imageDone = true;
      }
      continue;
    }
    if (flavor.EqualsLiteral(kFileMime)) {
      nsCOMPtr<nsIFile> file = do_QueryInterface(item);
      nsCOMPtr<nsIURI> uri;
      nsAutoCString spec;
      if (file && NS_SUCCEEDED(NS_NewFileURI(getter_AddRefs(uri), file)) &&
          NS_SUCCEEDED(uri->GetSpec(spec))) {
        mimes.AppendElement(nsLiteralCString(kUriListWire));
        payloads.AppendElement(spec);
      }
      continue;
    }
    void* data = nullptr;
    uint32_t length = 0;
    nsPrimitiveHelpers::CreateDataFromPrimitive(flavor, item, &data, &length);
    if (data) {
      mimes.AppendElement(flavor);
      payloads.AppendElement(nsDependentCSubstring(static_cast<char*>(data),
                                                   length));
      free(data);
    }
  }

  nsTArray<const char*> mimePointers;
  nsTArray<const uint8_t*> dataPointers;
  nsTArray<size_t> lengths;
  for (size_t i = 0; i < mimes.Length(); i++) {
    mimePointers.AppendElement(mimes[i].get());
    dataPointers.AppendElement(
        reinterpret_cast<const uint8_t*>(payloads[i].get()));
    lengths.AppendElement(payloads[i].Length());
  }
  const bool concealed =
      !StaticPrefs::clipboard_copyPrivateDataToClipboardCloudOrHistory() &&
      aTransferable->GetIsPrivateData();
  BandeHost::Copy(aWhichClipboard == kSelectionClipboard, concealed,
                  mimes.Length(), mimePointers.Elements(),
                  dataPointers.Elements(), lengths.Elements());
  mSequence++;
  return NS_OK;
}

nsTArray<nsCString> nsClipboard::OfferedTypes() const {
  nsTArray<nsCString> types;
  if (!BandeHost::IsActive()) {
    return types;
  }
  char buffer[4096];
  size_t length = BandeHost::ClipboardTypes(buffer, sizeof(buffer));
  for (const auto& type : nsDependentCSubstring(buffer, length).Split('\n')) {
    if (!type.IsEmpty()) {
      types.AppendElement(type);
    }
  }
  return types;
}

nsCString nsClipboard::WireType(const nsACString& aFlavor) const {
  nsTArray<nsCString> offered = OfferedTypes();
  if (aFlavor.EqualsLiteral(kTextMime)) {
    for (const char* wire : kTextWires) {
      if (offered.Contains(nsDependentCString(wire))) {
        return nsCString(wire);
      }
    }
    return nsCString();
  }
  if (aFlavor.EqualsLiteral(kFileMime)) {
    return offered.Contains(nsLiteralCString(kUriListWire))
               ? nsCString(kUriListWire)
               : nsCString();
  }
  if (IsImageFlavor(aFlavor)) {
    nsAutoCString wire(aFlavor.EqualsLiteral(kJPGImageMime)
                           ? nsLiteralCString(kJPEGImageMime)
                           : nsCString(aFlavor));
    if (offered.Contains(wire)) {
      return nsCString(wire);
    }
    return offered.Contains(nsLiteralCString(kPNGImageMime))
               ? nsCString(kPNGImageMime)
               : nsCString();
  }
  return offered.Contains(aFlavor) ? nsCString(aFlavor) : nsCString();
}

static nsCOMPtr<nsISupports> Convert(const nsACString& aFlavor,
                                     const nsACString& aWire,
                                     Span<const uint8_t> aData) {
  nsDependentCSubstring bytes(reinterpret_cast<const char*>(aData.Elements()),
                              aData.Length());
  if (aFlavor.EqualsLiteral(kTextMime) || aFlavor.EqualsLiteral(kHTMLMime)) {
    nsAutoString text;
    if (aFlavor.EqualsLiteral(kHTMLMime) && aData.Length() >= 2 &&
        ((aData[0] == 0xFF && aData[1] == 0xFE))) {
      text.Assign(reinterpret_cast<const char16_t*>(aData.Elements() + 2),
                  (aData.Length() - 2) / 2);
    } else {
      CopyUTF8toUTF16(bytes, text);
    }
    auto wrapper = MakeRefPtr<nsSupportsString>();
    wrapper->SetData(text);
    return nsCOMPtr<nsISupports>(wrapper);
  }
  if (aFlavor.EqualsLiteral(kFileMime)) {
    for (const auto& line : bytes.Split('\n')) {
      nsAutoCString spec(line);
      spec.Trim("\r ");
      if (spec.IsEmpty() || spec.First() == '#') {
        continue;
      }
      nsCOMPtr<nsIFile> file;
      if (NS_SUCCEEDED(NS_GetFileFromURLSpec(spec, getter_AddRefs(file)))) {
        return nsCOMPtr<nsISupports>(file);
      }
    }
    return nullptr;
  }
  if (IsImageFlavor(aFlavor)) {
    nsCOMPtr<nsIInputStream> stream;
    NS_NewByteInputStream(getter_AddRefs(stream),
                          Span(reinterpret_cast<const char*>(aData.Elements()),
                               aData.Length()),
                          NS_ASSIGNMENT_COPY);
    return nsCOMPtr<nsISupports>(stream);
  }
  nsCOMPtr<nsISupports> primitive;
  nsPrimitiveHelpers::CreatePrimitiveForData(
      aFlavor, aData.Elements(), aData.Length(), getter_AddRefs(primitive));
  return primitive;
}

void nsClipboard::AsyncGetNativeClipboardData(
    const nsACString& aFlavor, ClipboardType aWhichClipboard,
    GetNativeDataCallback&& aCallback) {
  nsCString wire = WireType(aFlavor);
  if (aWhichClipboard != kGlobalClipboard || wire.IsEmpty()) {
    aCallback(nsCOMPtr<nsISupports>{});
    return;
  }
  mRequests.AppendElement(Request{wire, nsCString(aFlavor), std::move(aCallback)});
  if (!BandeHost::Paste(wire.get())) {
    Request request = mRequests.PopLastElement();
    request.mCallback(nsCOMPtr<nsISupports>{});
  }
}

Result<nsCOMPtr<nsISupports>, nsresult> nsClipboard::GetNativeClipboardData(
    const nsACString& aFlavor, ClipboardType aWhichClipboard,
    uint64_t aThreshold) {
  bool done = false;
  Result<nsCOMPtr<nsISupports>, nsresult> result = nsCOMPtr<nsISupports>{};
  AsyncGetNativeClipboardData(
      aFlavor, aWhichClipboard,
      [&](Result<nsCOMPtr<nsISupports>, nsresult> aResult) {
        result = std::move(aResult);
        done = true;
      });
  SpinEventLoopUntil("nsClipboard::GetNativeClipboardData"_ns,
                     [&]() { return done; });
  return result;
}

void nsClipboard::Received(const nsACString& aMime, Span<const uint8_t> aData) {
  for (size_t i = 0; i < mRequests.Length(); i++) {
    if (mRequests[i].mMime == aMime) {
      Request request = std::move(mRequests[i]);
      mRequests.RemoveElementAt(i);
      request.mCallback(Convert(request.mFlavor, aMime, aData));
      return;
    }
  }
}

void nsClipboard::HandleData(const char* aMime, const uint8_t* aData,
                             size_t aLength) {
  if (sInstance && aMime) {
    RefPtr<nsClipboard> clipboard = sInstance;
    clipboard->Received(nsDependentCString(aMime), Span(aData, aLength));
  }
}

void nsClipboard::HandleChanged() {
  if (sInstance) {
    sInstance->mSequence++;
  }
}

nsresult nsClipboard::EmptyNativeClipboardData(ClipboardType aWhichClipboard) {
  if (BandeHost::IsActive()) {
    BandeHost::Copy(aWhichClipboard == kSelectionClipboard, false, 0, nullptr,
                    nullptr, nullptr);
    mSequence++;
  }
  return NS_OK;
}

Result<bool, nsresult> nsClipboard::HasNativeClipboardDataMatchingFlavors(
    const nsTArray<nsCString>& aFlavorList, ClipboardType aWhichClipboard) {
  if (aWhichClipboard != kGlobalClipboard) {
    return false;
  }
  for (const nsCString& flavor : aFlavorList) {
    if (!WireType(flavor).IsEmpty()) {
      return true;
    }
  }
  return false;
}

Result<int32_t, nsresult> nsClipboard::GetNativeClipboardSequenceNumber(
    ClipboardType aWhichClipboard) {
  return mSequence;
}
