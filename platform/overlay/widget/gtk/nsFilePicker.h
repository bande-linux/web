/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef widget_bande_nsFilePicker_h
#define widget_bande_nsFilePicker_h

#include "nsBaseFilePicker.h"
#include "nsCOMArray.h"
#include "nsIFile.h"
#include "nsString.h"

class nsFilePicker final : public nsBaseFilePicker {
 public:
  nsFilePicker() = default;

  NS_DECL_ISUPPORTS

  NS_IMETHOD Open(nsIFilePickerShownCallback* aCallback) override;
  NS_IMETHOD AppendFilter(const nsAString& aTitle,
                          const nsAString& aFilter) override;
  NS_IMETHOD SetDefaultString(const nsAString& aString) override;
  NS_IMETHOD GetDefaultString(nsAString& aString) override;
  NS_IMETHOD SetDefaultExtension(const nsAString& aExtension) override;
  NS_IMETHOD GetDefaultExtension(nsAString& aExtension) override;
  NS_IMETHOD GetFilterIndex(int32_t* aFilterIndex) override;
  NS_IMETHOD SetFilterIndex(int32_t aFilterIndex) override;
  NS_IMETHOD GetFile(nsIFile** aFile) override;
  NS_IMETHOD GetFileURL(nsIURI** aFileURL) override;
  NS_IMETHOD GetFiles(nsISimpleEnumerator** aFiles) override;

  static void HandleAnswer(uint32_t aRequest, bool aChosen,
                           const nsACString& aUris);

 protected:
  void InitNative(nsIWidget* aParent, const nsAString& aTitle) override;

 private:
  ~nsFilePicker() override = default;

  void Answered(bool aChosen, const nsACString& aUris);

  nsCOMPtr<nsIWidget> mParentWidget;
  nsString mTitle;
  nsCString mFilters;
  nsString mDefault;
  nsString mDefaultExtension;
  int32_t mFilterIndex = 0;
  nsCOMArray<nsIFile> mFiles;
  nsCString mFirstUri;
  nsCOMPtr<nsIFilePickerShownCallback> mCallback;
};

#endif
