/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "MediaKeysEventSourceFactory.h"

#include <unistd.h>

#include "BandeHost.h"
#include "mozilla/XREAppData.h"
#include "mozilla/dom/MediaControlKeySource.h"
#include "mozilla/dom/MediaControlUtils.h"
#include "mozilla/dom/MediaSession.h"
#include "nsAppRunner.h"
#include "nsMimeTypes.h"
#include "nsPrintfCString.h"

namespace mozilla::widget {

static constexpr uint32_t kActionPlay = 1;
static constexpr uint32_t kActionPause = 2;
static constexpr uint32_t kActionPlayPause = 3;
static constexpr uint32_t kActionStop = 4;
static constexpr uint32_t kActionNext = 5;
static constexpr uint32_t kActionPrevious = 6;
static constexpr uint32_t kActionSeekBy = 7;
static constexpr uint32_t kActionSeekTo = 8;
static constexpr uint32_t kActionRaise = 9;

static constexpr uint32_t kKeyPlay = 1;
static constexpr uint32_t kKeyPause = 2;
static constexpr uint32_t kKeyNext = 4;
static constexpr uint32_t kKeyPrevious = 8;
static constexpr uint32_t kKeySeek = 16;

class BandeMediaKeys final : public dom::MediaControlKeySource {
 public:
  NS_INLINE_DECL_REFCOUNTING(BandeMediaKeys, override)

  bool Open() override {
    if (mOpened) {
      return true;
    }
    nsAutoCString remoting(gAppData && gAppData->remotingName
                               ? gAppData->remotingName
                               : "org.bande.Browser");
    nsPrintfCString name("%s.instance%d", remoting.get(), int(getpid()));
    mOpened = BandeHost::MprisOpen(name.get(), "Bande Browser", remoting.get());
    if (mOpened) {
      sCurrent = this;
    }
    return mOpened;
  }

  void Close() override {
    if (mOpened) {
      BandeHost::MprisClose();
      mOpened = false;
    }
    if (sCurrent == this) {
      sCurrent = nullptr;
    }
    dom::MediaControlKeySource::Close();
  }

  bool IsOpened() const override { return mOpened; }

  void SetPlaybackState(dom::MediaSessionPlaybackState aState) override {
    dom::MediaControlKeySource::SetPlaybackState(aState);
    if (mOpened) {
      BandeHost::MprisStatus(
          aState == dom::MediaSessionPlaybackState::Playing  ? 2
          : aState == dom::MediaSessionPlaybackState::Paused ? 1
                                                             : 0);
    }
  }

  void SetMediaMetadata(const dom::MediaMetadataBase& aMetadata) override {
    if (!mOpened) {
      return;
    }
    nsCOMPtr<nsIInputStream> stream;
    uint32_t size = 0;
    char* data = nullptr;
    for (const dom::MediaImageData& image : aMetadata.mArtwork) {
      if (image.mDataSurface &&
          NS_SUCCEEDED(dom::GetEncodedImageBuffer(image.mDataSurface,
                                                  nsLiteralCString(IMAGE_PNG),
                                                  getter_AddRefs(stream), &size,
                                                  &data)) &&
          data && size) {
        break;
      }
      data = nullptr;
      size = 0;
    }
    BandeHost::MprisTrack(NS_ConvertUTF16toUTF8(aMetadata.mTitle).get(),
                          NS_ConvertUTF16toUTF8(aMetadata.mArtist).get(),
                          NS_ConvertUTF16toUTF8(aMetadata.mAlbum).get(),
                          reinterpret_cast<const uint8_t*>(data), size);
  }

  void SetSupportedMediaKeys(const MediaKeysArray& aSupportedKeys) override {
    uint32_t keys = 0;
    for (const dom::MediaControlKey& key : aSupportedKeys) {
      switch (key) {
        case dom::MediaControlKey::Play:
          keys |= kKeyPlay;
          break;
        case dom::MediaControlKey::Pause:
          keys |= kKeyPause;
          break;
        case dom::MediaControlKey::Nexttrack:
          keys |= kKeyNext;
          break;
        case dom::MediaControlKey::Previoustrack:
          keys |= kKeyPrevious;
          break;
        case dom::MediaControlKey::Seekto:
          keys |= kKeySeek;
          break;
        default:
          break;
      }
    }
    if (mOpened) {
      BandeHost::MprisKeys(keys);
    }
  }

  void SetPositionState(const Maybe<dom::PositionState>& aState) override {
    if (!mOpened) {
      return;
    }
    if (aState) {
      BandeHost::MprisPosition(aState->mDuration,
                               aState->CurrentPlaybackPosition(),
                               aState->mPlaybackRate);
    } else {
      BandeHost::MprisPosition(0.0, 0.0, 1.0);
    }
  }

  static void HandleAction(uint32_t aAction, double aValue) {
    RefPtr<BandeMediaKeys> keys = sCurrent;
    if (!keys) {
      return;
    }
    Maybe<dom::MediaControlAction> action;
    switch (aAction) {
      case kActionPlay:
        action.emplace(dom::MediaControlKey::Play);
        break;
      case kActionPause:
        action.emplace(dom::MediaControlKey::Pause);
        break;
      case kActionPlayPause:
        action.emplace(dom::MediaControlKey::Playpause);
        break;
      case kActionStop:
        action.emplace(dom::MediaControlKey::Stop);
        break;
      case kActionNext:
        action.emplace(dom::MediaControlKey::Nexttrack);
        break;
      case kActionPrevious:
        action.emplace(dom::MediaControlKey::Previoustrack);
        break;
      case kActionRaise:
        action.emplace(dom::MediaControlKey::Focus);
        break;
      case kActionSeekTo:
        action.emplace(dom::MediaControlKey::Seekto,
                       dom::MediaControlActionParams(aValue, false));
        break;
      case kActionSeekBy:
        if (aValue >= 0.0) {
          action.emplace(dom::MediaControlKey::Seekforward,
                         dom::MediaControlActionParams(aValue));
        } else {
          action.emplace(dom::MediaControlKey::Seekbackward,
                         dom::MediaControlActionParams(-aValue));
        }
        break;
      default:
        return;
    }
    for (auto& listener : keys->mListeners.Clone()) {
      listener->OnActionPerformed(*action);
    }
  }

 private:
  ~BandeMediaKeys() override { Close(); }

  bool mOpened = false;
  static BandeMediaKeys* sCurrent;
};

BandeMediaKeys* BandeMediaKeys::sCurrent = nullptr;

void HandleBandeMediaAction(uint32_t aAction, double aValue) {
  BandeMediaKeys::HandleAction(aAction, aValue);
}

mozilla::dom::MediaControlKeySource* CreateMediaControlKeySource() {
  return BandeHost::IsActive() ? new BandeMediaKeys() : nullptr;
}

}  // namespace mozilla::widget
