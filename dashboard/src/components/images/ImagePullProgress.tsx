// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import React, { useEffect, useState, useRef } from 'react';
import {
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalBody,
  ModalFooter,
  Button,
  VStack,
  Text,
  Progress,
  HStack,
  Spinner,
} from '@chakra-ui/react';
import { apiClient } from '../../api/client';
import type { PullProgressEvent } from '../../api/types';

interface ImagePullProgressProps {
  isOpen: boolean;
  onClose: () => void;
  imageName: string;
  tag: string;
  onComplete: () => void;
}

export function ImagePullProgress({
  isOpen,
  onClose,
  imageName,
  tag,
  onComplete,
}: ImagePullProgressProps) {
  const [status, setStatus] = useState<string>('');
  const [progress, setProgress] = useState<number>(0);
  const [error, setError] = useState<string | null>(null);
  const [isComplete, setIsComplete] = useState(false);
  const cleanupRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    if (!isOpen) return;

    setStatus('Starting pull...');
    setProgress(0);
    setError(null);
    setIsComplete(false);

    // Start SSE stream for progress
    cleanupRef.current = apiClient.streamPullProgress(
      imageName,
      tag,
      (event: PullProgressEvent) => {
        setStatus(event.status);

        // Calculate progress percentage
        if (event.progress_detail) {
          const { current, total } = event.progress_detail;
          if (total > 0) {
            setProgress(Math.round((current / total) * 100));
          }
        }

        // Check if complete
        if (event.status === 'Pull complete') {
          setIsComplete(true);
          setTimeout(() => {
            onComplete();
            onClose();
          }, 1000);
        } else if (event.status === 'Pull failed') {
          setError('Pull failed - check logs for details');
          setIsComplete(true);
        }
      },
      (err) => {
        console.error('SSE error:', err);
        setError('Connection error - pull may still be in progress');
      }
    );

    return () => {
      cleanupRef.current?.();
    };
  }, [isOpen, imageName, tag, onComplete, onClose]);

  return (
    <Modal isOpen={isOpen} onClose={onClose} closeOnOverlayClick={!isComplete}>
      <ModalOverlay />
      <ModalContent>
        <ModalHeader>Pulling Image</ModalHeader>

        <ModalBody>
          <VStack align="stretch" spacing={4}>
            <HStack>
              <Text fontWeight="bold">{imageName}</Text>
              <Text color="gray.500">:{tag}</Text>
            </HStack>

            {error ? (
              <Text color="red.500">{error}</Text>
            ) : (
              <>
                <HStack>
                  {progress > 0 && !isComplete ? <Spinner size="sm" /> : null}
                  <Text fontSize="sm" color="gray.600">
                    {status}
                  </Text>
                </HStack>

                {progress > 0 && (
                  <VStack align="stretch" spacing={2}>
                    <Progress value={progress} size="lg" colorScheme="blue" />
                    <Text fontSize="xs" color="gray.500" textAlign="right">
                      {progress}%
                    </Text>
                  </VStack>
                )}

                {isComplete && !error && (
                  <Text color="green.500" fontWeight="bold">
                    ✓ Pull complete!
                  </Text>
                )}
              </>
            )}
          </VStack>
        </ModalBody>

        <ModalFooter>
          {isComplete ? (
            <Button colorScheme="blue" onClick={onClose}>
              Close
            </Button>
          ) : (
            <Button variant="ghost" onClick={onClose} isDisabled>
              Pulling...
            </Button>
          )}
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
}
