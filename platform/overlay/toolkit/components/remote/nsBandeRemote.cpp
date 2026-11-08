/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "nsBandeRemote.h"

#include "RemoteUtils.h"
#include "mozilla/Base64.h"
#include "mozilla/Span.h"
#include "mozilla/TextUtils.h"
#include "mozilla/XREAppData.h"
#include "mozilla/widget/BandeHost.h"
#include "nsAppRunner.h"
#include "nsPrintfCString.h"

using namespace mozilla;
using mozilla::widget::BandeHost;

static constexpr size_t kMaximumBusName = 255;

static nsBandeRemoteServer* sServer = nullptr;

bool BandeRemoteAddressFor(const char* aProfile, BandeRemoteAddress& aAddress) {
  if (!gAppData || !aProfile || !*aProfile) {
    return false;
  }
  nsAutoCString appName;
  gAppData->GetDBusAppName(appName);
  nsAutoCString profileName;
  if (NS_FAILED(Base64Encode(nsDependentCString(aProfile), profileName))) {
    return false;
  }
  XREAppData::SanitizeNameForDBus(profileName);
  if (profileName.IsEmpty() || IsAsciiDigit(profileName.First())) {
    profileName.AssignLiteral("default");
  }
  aAddress.mName =
      nsPrintfCString("org.mozilla.%s.%s", appName.get(), profileName.get());
  if (aAddress.mName.Length() > kMaximumBusName) {
    aAddress.mName.Truncate(kMaximumBusName);
  }
  aAddress.mPath = nsPrintfCString("/org/mozilla/%s/Remote", appName.get());
  aAddress.mInterface = nsPrintfCString("org.mozilla.%s", appName.get());
  return true;
}

nsBandeRemoteServer::~nsBandeRemoteServer() { Shutdown(); }

nsresult nsBandeRemoteServer::Startup(const char* aAppName,
                                      const char* aProfileName) {
  BandeRemoteAddress address;
  if (!BandeRemoteAddressFor(aProfileName, address)) {
    return NS_ERROR_INVALID_ARG;
  }
  sServer = this;
  if (!BandeHost::RemoteServe(address.mName.get(), address.mPath.get(),
                              address.mInterface.get(), Handle)) {
    sServer = nullptr;
    return NS_ERROR_FAILURE;
  }
  return NS_OK;
}

void nsBandeRemoteServer::Shutdown() {
  if (sServer == this) {
    BandeHost::RemoteStop();
    sServer = nullptr;
  }
}

void nsBandeRemoteServer::Handle(const uint8_t* aCommand, size_t aLength) {
  if (sServer && aCommand && aLength) {
    sServer->HandleCommandLine(
        Span(reinterpret_cast<const char*>(aCommand), aLength), 0);
  }
}

nsresult nsBandeRemoteClient::SendCommandLine(const char* aProgram,
                                              const char* aProfile,
                                              int32_t argc, const char** argv,
                                              bool aRaise) {
  BandeRemoteAddress address;
  if (!BandeRemoteAddressFor(aProfile, address)) {
    return NS_ERROR_INVALID_ARG;
  }
  int length = 0;
  UniquePtr<char[]> commandLine = ConstructCommandLine(
      argc, argv,
      mStartupToken.IsEmpty() ? nullptr : mStartupToken.get(), &length);
  if (!commandLine || length <= 0) {
    return NS_ERROR_FAILURE;
  }
  return BandeHost::RemoteSend(
             address.mName.get(), address.mPath.get(),
             address.mInterface.get(),
             reinterpret_cast<const uint8_t*>(commandLine.get()),
             size_t(length))
             ? NS_OK
             : NS_ERROR_NOT_AVAILABLE;
}
