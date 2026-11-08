/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "nsFilePicker.h"

#include "BandeHost.h"
#include "nsArrayEnumerator.h"
#include "nsNetUtil.h"
#include "nsTHashMap.h"
#include "nsWindow.h"

using namespace mozilla;
using namespace mozilla::widget;

NS_IMPL_ISUPPORTS(nsFilePicker, nsIFilePicker)

static nsTHashMap<uint32_t, RefPtr<nsFilePicker>>& Pending() {
  static nsTHashMap<uint32_t, RefPtr<nsFilePicker>> sPending;
  return sPending;
}

void nsFilePicker::InitNative(nsIWidget* aParent, const nsAString& aTitle) {
  mParentWidget = aParent;
  mTitle = aTitle;
}

NS_IMETHODIMP
nsFilePicker::AppendFilter(const nsAString& aTitle, const nsAString& aFilter) {
  if (aFilter.EqualsLiteral("..apps")) {
    return NS_OK;
  }
  nsAutoString patterns(aFilter);
  patterns.StripWhitespace();
  mFilters.Append(NS_ConvertUTF16toUTF8(aTitle));
  mFilters.Append('\t');
  mFilters.Append(NS_ConvertUTF16toUTF8(patterns));
  mFilters.Append('\n');
  return NS_OK;
}

NS_IMETHODIMP
nsFilePicker::SetDefaultString(const nsAString& aString) {
  mDefault = aString;
  return NS_OK;
}

NS_IMETHODIMP
nsFilePicker::GetDefaultString(nsAString& aString) {
  aString = mDefault;
  return NS_OK;
}

NS_IMETHODIMP
nsFilePicker::SetDefaultExtension(const nsAString& aExtension) {
  mDefaultExtension = aExtension;
  return NS_OK;
}

NS_IMETHODIMP
nsFilePicker::GetDefaultExtension(nsAString& aExtension) {
  aExtension = mDefaultExtension;
  return NS_OK;
}

NS_IMETHODIMP
nsFilePicker::GetFilterIndex(int32_t* aFilterIndex) {
  *aFilterIndex = mFilterIndex;
  return NS_OK;
}

NS_IMETHODIMP
nsFilePicker::SetFilterIndex(int32_t aFilterIndex) {
  mFilterIndex = aFilterIndex;
  return NS_OK;
}

NS_IMETHODIMP
nsFilePicker::Open(nsIFilePickerShownCallback* aCallback) {
  if (!BandeHost::IsActive() || mCallback) {
    return NS_ERROR_NOT_AVAILABLE;
  }
  uint64_t window = 0;
  for (nsIWidget* widget = mParentWidget; widget && !window;
       widget = widget->GetParent()) {
    if (nsWindow* bande = nsWindow::FromWidget(widget)) {
      window = bande->BandeWindow();
    }
  }
  nsAutoCString folder;
  if (mDisplayDirectory) {
    nsAutoString path;
    mDisplayDirectory->GetPath(path);
    CopyUTF16toUTF8(path, folder);
  }
  static uint32_t sNextRequest = 1;
  const uint32_t request = sNextRequest++;
  mCallback = aCallback;
  Pending().InsertOrUpdate(request, this);
  BandeHost::ChooseFiles(window, request, uint32_t(mMode),
                         NS_ConvertUTF16toUTF8(mTitle).get(), mFilters.get(),
                         uint32_t(std::max(mFilterIndex, 0)),
                         NS_ConvertUTF16toUTF8(mDefault).get(), folder.get());
  return NS_OK;
}

void nsFilePicker::HandleAnswer(uint32_t aRequest, bool aChosen,
                                const nsACString& aUris) {
  RefPtr<nsFilePicker> picker;
  if (Pending().Remove(aRequest, getter_AddRefs(picker)) && picker) {
    picker->Answered(aChosen, aUris);
  }
}

void nsFilePicker::Answered(bool aChosen, const nsACString& aUris) {
  mFiles.Clear();
  mFirstUri.Truncate();
  for (const auto& uri : aUris.Split('\n')) {
    nsCOMPtr<nsIFile> file;
    if (!uri.IsEmpty() &&
        NS_SUCCEEDED(NS_GetFileFromURLSpec(nsCString(uri),
                                           getter_AddRefs(file)))) {
      if (mFirstUri.IsEmpty()) {
        mFirstUri = uri;
      }
      mFiles.AppendObject(file);
    }
  }
  nsIFilePicker::ResultCode result = nsIFilePicker::returnCancel;
  if (aChosen && mFiles.Count() > 0) {
    result = nsIFilePicker::returnOK;
    bool exists = false;
    if (mMode == nsIFilePicker::modeSave &&
        NS_SUCCEEDED(mFiles[0]->Exists(&exists)) && exists) {
      result = nsIFilePicker::returnReplace;
    }
  }
  nsCOMPtr<nsIFilePickerShownCallback> callback = std::move(mCallback);
  if (callback) {
    callback->Done(result);
  }
}

NS_IMETHODIMP
nsFilePicker::GetFile(nsIFile** aFile) {
  *aFile = nullptr;
  if (mFiles.Count() == 0) {
    return NS_OK;
  }
  return mFiles[0]->Clone(aFile);
}

NS_IMETHODIMP
nsFilePicker::GetFileURL(nsIURI** aFileURL) {
  *aFileURL = nullptr;
  if (mFirstUri.IsEmpty()) {
    return NS_OK;
  }
  return NS_NewURI(aFileURL, mFirstUri);
}

NS_IMETHODIMP
nsFilePicker::GetFiles(nsISimpleEnumerator** aFiles) {
  if (mMode != nsIFilePicker::modeOpenMultiple) {
    return NS_ERROR_FAILURE;
  }
  return NS_NewArrayEnumerator(aFiles, mFiles, NS_GET_IID(nsIFile));
}
