import numpy as np

DATA_PATH = '/Users/vitalyvorobyev/vision/data/stereo'

def main():
    """ Run me """
    data = np.load(f'{DATA_PATH}/out/parameters.npz')
    print(list(data.keys()))

    print(data['BoardSize'], data['SquareSize'])
    print(data['Transformation'])
    print(data['R_Extrinsics'])
    print(data['L_Extrinsics'])
    print(data['Essential'])
    print(data['Fundamental'])

if __name__ == '__main__':
    main()
